//! Native `pw::metadata::Metadata` read of the `default` metadata object's
//! `default.audio.sink`/`default.audio.source` keys (#432, Gap 3 of epic
//! #8's closing sweep) — replaces `pactl get-default-sink`/`get-default-source`
//! (`pactl/routing.rs`) with a direct read of the same PipeWire object
//! `pactl` itself reads to answer those commands.
//!
//! This is this codebase's first use of a `Metadata` proxy — every earlier
//! native module (`pw_link_native.rs`, `pw_mixer_native.rs`,
//! `pw_virtual_device_native.rs`) only ever bound `Node`/`Port`/`Link`. A
//! PipeWire server can expose more than one metadata object (e.g.
//! `route-settings`); the one carrying `default.audio.sink`/
//! `default.audio.source` is identified by its own `metadata.name` property
//! being `"default"` — this module's registry filter checks that, not just
//! `ObjectType::Metadata`.
//!
//! Structurally identical `Connection` skeleton to `pw_mixer_native.rs`
//! (`ThreadLoopRc`/`ContextRc`/`CoreRc`/`RegistryRc`, `ManuallyDrop`,
//! process-wide `OnceLock`, PD-042's reasoning) but the registry listener's
//! job is just to find the one `default` metadata global's id — there's no
//! ongoing index to maintain the way `Node` name lookups need one, so the
//! `Connection` holds a single resolved id instead of a `HashMap`.
//!
//! ## Reading a property is not `enum_params`
//!
//! Unlike `pw_mixer_native.rs::query_props` (which must *trigger* a read via
//! `enum_params` and then wait for a `param` event), a bound `Metadata`
//! proxy's `property` listener fires once per already-set key immediately
//! upon `add_listener_local().register()` — PipeWire's metadata objects push
//! their whole current key set to every new listener as a "dump", not on
//! request. So [`Connection::read_default`] just registers the listener and
//! waits for the specific key it wants to show up, with no separate trigger
//! call.
//!
//! ## The value is a JSON object, not a bare name
//!
//! `default.audio.sink`/`default.audio.source` values are JSON strings of
//! the form `{"name":"<node.name>"}` (PipeWire's own convention for
//! structured metadata values), not the bare node name `pactl`'s own command
//! output is. [`extract_name`] pulls the `name` field back out.

use crate::backend::BackendError;
use pipewire as pw;
use pw::context::ContextRc;
use pw::core::CoreRc;
use pw::metadata::Metadata;
use pw::permissions::PermissionFlags;
use pw::properties::PropertiesBox;
use pw::registry::{GlobalObject, RegistryRc};
use pw::thread_loop::ThreadLoopRc;
use pw::types::ObjectType;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{self};
use std::sync::OnceLock;
use std::time::Duration;

const INITIAL_INDEX_TIMEOUT: Duration = Duration::from_millis(500);
/// How long [`Connection::read_default`] waits for the `property` event
/// carrying the requested key — same reasoning as
/// `pw_mixer_native::PROPS_QUERY_TIMEOUT`: a genuine round trip against this
/// process's own already-open connection, not a network call, so 500ms is
/// generous rather than tight.
const PROPERTY_READ_TIMEOUT: Duration = Duration::from_millis(500);

struct Connection {
    _thread_loop: ManuallyDrop<ThreadLoopRc>,
    _context: ManuallyDrop<ContextRc>,
    _core: ManuallyDrop<CoreRc>,
    registry: ManuallyDrop<RegistryRc>,
    _listener: ManuallyDrop<pw::registry::Listener>,
    default_metadata_id: Option<u32>,
}

// SAFETY: identical contract to `pw_mixer_native.rs::Connection` — every
// touch of the `pw`-owned fields happens either during setup (this thread,
// before `start()` returns) or from callbacks pw's own thread loop invokes
// on its internal thread, never concurrently; `registry` is read afterward
// (under `thread_loop.lock()`) to `bind()` a `Metadata` proxy.
// `default_metadata_id` is a plain `Option<u32>`, resolved once and never
// mutated again after `start()` returns.
unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

impl Connection {
    fn start() -> Option<Self> {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);

        // SAFETY: `pw::init()` has just been called above (exactly once,
        // process-wide, via `PW_INIT`).
        let thread_loop = unsafe { ThreadLoopRc::new(Some("pipe-deck-metadata"), None) }.ok()?;
        thread_loop.start();

        let (tx, rx) = mpsc::channel::<u32>();

        let (context, core, registry, listener) = {
            let _lock = thread_loop.lock();
            let context = ContextRc::new(&thread_loop, None).ok()?;
            let core = context.connect_rc(None).ok()?;
            let registry = core.get_registry_rc().ok()?;

            let listener = registry
                .add_listener_local()
                .global(move |global| {
                    if global.type_ != ObjectType::Metadata {
                        return;
                    }
                    if metadata_name(global).as_deref() != Some("default") {
                        return;
                    }
                    let _ = tx.send(global.id);
                })
                .register();

            (context, core, registry, listener)
        };

        let default_metadata_id = rx.recv_timeout(INITIAL_INDEX_TIMEOUT).ok();

        Some(Self {
            _thread_loop: ManuallyDrop::new(thread_loop),
            _context: ManuallyDrop::new(context),
            _core: ManuallyDrop::new(core),
            registry: ManuallyDrop::new(registry),
            _listener: ManuallyDrop::new(listener),
            default_metadata_id,
        })
    }

    /// Binds the `default` metadata object and waits for a `property` event
    /// naming `key`, returning its raw JSON-string value (or `Ok(None)` if
    /// the key isn't currently set — a legitimate "no default configured"
    /// state, not an error). See this module's doc comment for why no
    /// separate trigger call is needed: registering the listener alone
    /// causes PipeWire to dump every currently-set key.
    fn read_default(&self, key: &'static str) -> Result<Option<String>, BackendError> {
        let metadata_id = self
            .default_metadata_id
            .ok_or_else(|| BackendError::Message("default metadata object not found".into()))?;
        let global = GlobalObject {
            id: metadata_id,
            permissions: PermissionFlags::empty(),
            type_: ObjectType::Metadata,
            version: 0,
            props: None::<PropertiesBox>,
        };
        let (tx, rx) = mpsc::channel::<Option<String>>();

        let (metadata, listener) = {
            let _lock = self._thread_loop.lock();
            let metadata: Metadata = self.registry.bind(&global).map_err(|_| {
                BackendError::Message(format!("failed to bind metadata object {metadata_id}"))
            })?;
            let listener = metadata
                .add_listener_local()
                .property(move |_subject, event_key, _type_, value| {
                    if event_key == Some(key) {
                        let _ = tx.send(value.map(str::to_string));
                    }
                    0
                })
                .register();
            (metadata, listener)
        };

        // No key at all currently set is indistinguishable, from this
        // listener's perspective, from "the key just hasn't arrived yet" —
        // both look like silence on `rx`. Since the dump happens
        // synchronously on registration, giving the full timeout a chance to
        // elapse before concluding "not set" is correct, not just a
        // fallback: a real network round trip isn't in play here, only the
        // dispatch of already-buffered events.
        let result = rx.recv_timeout(PROPERTY_READ_TIMEOUT).ok().flatten();

        {
            let _lock = self._thread_loop.lock();
            drop(listener);
            drop(metadata);
        }

        Ok(result)
    }
}

fn metadata_name(
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) -> Option<String> {
    global
        .props?
        .get("metadata.name")
        .map(|name| name.to_string())
}

/// Pulls the `name` field back out of a `default.audio.sink`/
/// `default.audio.source` value, which PipeWire stores as a JSON object of
/// the form `{"name":"<node.name>"}` rather than a bare string.
fn extract_name(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value.get("name")?.as_str().map(str::to_string)
}

static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();

fn connection() -> Option<&'static Connection> {
    CONNECTION.get_or_init(Connection::start).as_ref()
}

/// Native equivalent of `pactl/routing.rs::get_default_sink_name`. `None` if
/// the native connection couldn't be established or the `default` metadata
/// object wasn't found (fall back to `pactl get-default-sink`);
/// `Some(Ok(None))` if the connection is up but no default sink is
/// currently configured.
pub fn default_sink_name() -> Option<Result<Option<String>, BackendError>> {
    let conn = connection()?;
    Some(
        conn.read_default("default.audio.sink")
            .map(|raw| raw.as_deref().and_then(extract_name)),
    )
}

/// Native equivalent of `pactl/routing.rs::get_default_source_name`.
pub fn default_source_name() -> Option<Result<Option<String>, BackendError>> {
    let conn = connection()?;
    Some(
        conn.read_default("default.audio.source")
            .map(|raw| raw.as_deref().and_then(extract_name)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_name_pulls_name_field_out_of_json_value() {
        assert_eq!(
            extract_name(r#"{"name":"alsa_output.pci-0000_00_1f.3.analog-stereo"}"#),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo".to_string())
        );
    }

    #[test]
    fn extract_name_returns_none_for_malformed_value() {
        assert_eq!(extract_name("not json"), None);
        assert_eq!(extract_name(r#"{"other":"field"}"#), None);
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. Cross-checks this
    //! module's native read against `pactl get-default-sink`/
    //! `get-default-source`'s own output — an independent pipeline (a fresh
    //! shell-out + stdout trim), so a bug in this module's JSON-unwrapping
    //! or metadata-object discovery would show up as a real mismatch here,
    //! not just an internal self-consistency check.
    use super::*;
    use crate::sysproc;

    fn pactl_default(args: &[&str]) -> Option<String> {
        let output = sysproc::command("pactl").args(args).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    #[test]
    #[ignore]
    fn reads_default_sink_and_source_natively_matching_pactls_own_readback() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let native_sink = default_sink_name()
            .expect("expected the native path to run, not fall back to the CLI")
            .expect("native default sink read should succeed");
        let pactl_sink = pactl_default(&["get-default-sink"]);
        assert_eq!(
            native_sink, pactl_sink,
            "native default sink should match pactl's own readback"
        );

        let native_source = default_source_name()
            .expect("expected the native path to run, not fall back to the CLI")
            .expect("native default source read should succeed");
        let pactl_source = pactl_default(&["get-default-source"]);
        assert_eq!(
            native_source, pactl_source,
            "native default source should match pactl's own readback"
        );
    }
}
