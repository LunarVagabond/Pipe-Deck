//! Native `pw::node::Node::set_param(Props)` volume/mute control (#413, step
//! 4 of #8's native `pipewire-rs` migration — scoped to the volume/mute
//! slice only, per the ticket's own suggestion to split rather than port
//! `pactl/*`'s ~1750 lines in one PR). Replaces `pactl set-sink-volume`/
//! `set-source-volume`/`set-sink-mute`/`set-source-mute` for **device**
//! volume/mute with a direct `SPA_PROP_channelVolumes`/`SPA_PROP_mute` push
//! over this process's own persistent PipeWire connection — the same
//! `set_param`-against-a-bound-`Node`-proxy technique `pipewire/native_host.rs`
//! already uses for live effect params, applied here to the standard mixer
//! props every sink/source node exposes rather than a filter-chain's custom
//! control ports.
//!
//! Volume/mute *reads* (`sink_volume_percent`/`sink_mute_state`, #424) go
//! through `Node::enum_params` + Pod deserialization (`query_props`) —
//! this codebase's first use of that direction; every earlier native
//! `set_param` call site, including this module's own writes, is write-only.
//! See PD-043/PD-046.
//!
//! **Stream volume/mute (`set_stream_volume`/`set_stream_mute`, sink-inputs/
//! source-outputs) is still out of scope** — streams are resolved to a
//! `pactl` sink-input/source-output index via `pactl/parse.rs`'s own text
//! parsing, a separate porting problem from device volume/mute (#413's
//! original ticket text called out sink-input routing/moves as needing its
//! own design pass).
//!
//! Structurally parallel to `pw_link_native.rs` (own process-wide
//! `OnceLock` connection, since callers span `pactl/mixer.rs`,
//! `pactl/virtual.rs`, and `live.rs` with no single shared owner — PD-042's
//! reasoning applies identically here) but only needs a `node.name -> id`
//! index, not the port/link tracking port-linking requires.
//!
//! ## The cubic volume curve
//!
//! PipeWire's `SPA_PROP_channelVolumes` values are raw linear amplitude
//! multipliers (`1.0` = unity gain), but the percent this codebase's UI (and
//! `pactl`, which is what set volume before this) works in is a *cubic*
//! curve — confirmed against `pactl/parse.rs`'s own test fixtures
//! (`"64860 /  99% /  -0.27 dB"`: `0.99³ ≈ 0.9703`, `20·log10(0.9703) ≈
//! -0.261 dB`, matching within rounding). Pushing `percent / 100.0` directly
//! as the linear amplitude instead of `(percent / 100.0)³` would make every
//! volume level sound audibly louder than the exact same percent set via
//! `pactl` — a real, easy-to-miss regression, not a cosmetic one, if the
//! native and CLI paths ever disagree on loudness for what the UI displays
//! as the same percentage. `linear_amplitude` below is the single place this
//! conversion happens.

use crate::backend::BackendError;
use pipewire as pw;
use pw::context::ContextRc;
use pw::core::CoreRc;
use pw::permissions::PermissionFlags;
use pw::properties::PropertiesBox;
use pw::registry::{GlobalObject, RegistryRc};
use pw::spa::param::ParamType;
use pw::spa::pod::deserialize::PodDeserializer;
use pw::spa::pod::serialize::{PodSerialize, PodSerializer, SerializeSuccess};
use pw::spa::pod::{Pod, PropertyFlags, Value, ValueArray};
use pw::spa::sys as spa_sys;
use pw::spa::utils::dict::DictRef;
use pw::thread_loop::ThreadLoopRc;
use pw::types::ObjectType;
use std::collections::HashMap;
use std::io::Cursor;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const INITIAL_INDEX_TIMEOUT: Duration = Duration::from_millis(500);
/// How long `query_props` waits for the `param` event `enum_params` triggers
/// — a genuine async round trip (unlike `set_props`, which is fire-and-forget),
/// so unlike every other timeout in this module this one gates a real network
/// (well, unix-socket) round trip with the server, not just an initial-index
/// grace period. 500ms is generous for a same-machine round trip; a node that
/// doesn't answer in that window almost certainly isn't going to.
const PROPS_QUERY_TIMEOUT: Duration = Duration::from_millis(500);

struct Connection {
    _thread_loop: ManuallyDrop<ThreadLoopRc>,
    _context: ManuallyDrop<ContextRc>,
    _core: ManuallyDrop<CoreRc>,
    registry: ManuallyDrop<RegistryRc>,
    _listener: ManuallyDrop<pw::registry::Listener>,
    node_ids: Arc<Mutex<HashMap<String, u32>>>,
}

// SAFETY: identical contract to `pw_link_native.rs::Connection` — every
// touch of the `pw`-owned fields happens either during setup (this thread,
// before `start()` returns) or from callbacks pw's own thread loop invokes
// on its internal thread, never concurrently; `registry` is read afterward
// (under `thread_loop.lock()`) to `bind()` a `Node` proxy, matching
// `native_host.rs`'s identical justification. `node_ids` is a plain
// `Arc<Mutex<..>>`.
unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

impl Connection {
    fn start() -> Option<Self> {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);

        // SAFETY: `pw::init()` has just been called above (exactly once,
        // process-wide, via `PW_INIT`).
        let thread_loop = unsafe { ThreadLoopRc::new(Some("pipe-deck-mixer"), None) }.ok()?;
        thread_loop.start();

        let (tx, rx) = mpsc::channel::<(u32, String)>();

        let (context, core, registry, listener) = {
            let _lock = thread_loop.lock();
            let context = ContextRc::new(&thread_loop, None).ok()?;
            let core = context.connect_rc(None).ok()?;
            let registry = core.get_registry_rc().ok()?;

            let listener = registry
                .add_listener_local()
                .global(move |global| {
                    if global.type_ != ObjectType::Node {
                        return;
                    }
                    let Some(name) = node_name(global) else {
                        return;
                    };
                    let _ = tx.send((global.id, name));
                })
                .register();

            (context, core, registry, listener)
        };

        let node_ids: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
        let assembler_node_ids = node_ids.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        thread::spawn(move || run_assembler(rx, assembler_node_ids, ready_tx));
        let _ = ready_rx.recv_timeout(INITIAL_INDEX_TIMEOUT);

        Some(Self {
            _thread_loop: ManuallyDrop::new(thread_loop),
            _context: ManuallyDrop::new(context),
            _core: ManuallyDrop::new(core),
            registry: ManuallyDrop::new(registry),
            _listener: ManuallyDrop::new(listener),
            node_ids,
        })
    }

    fn node_id(&self, name: &str) -> Option<u32> {
        self.node_ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(name)
            .copied()
    }

    /// Binds a `Node` proxy for `node_id` and pushes `props` as a
    /// `SPA_PARAM_Props` update, exactly matching
    /// `native_host.rs::set_param`'s bind/set/drop-under-one-lock shape (see
    /// that function's own doc comment for why the proxy's bind, use, and
    /// drop all need to happen inside the same `thread_loop.lock()` guard —
    /// letting the bound proxy outlive the lock trips PipeWire's own
    /// thread-safety assertions).
    fn set_props<P: PodSerialize>(&self, node_id: u32, props: &P) -> Result<(), BackendError> {
        let mut bytes = Vec::new();
        PodSerializer::serialize(Cursor::new(&mut bytes), props).map_err(|error| {
            BackendError::Message(format!("failed to build Props pod: {error:?}"))
        })?;
        let pod = Pod::from_bytes(&bytes).ok_or_else(|| {
            BackendError::Message("serialized Props pod bytes were malformed".into())
        })?;

        // `bind()` only reads `id` and (via `type_.client_version()`)
        // `type_` — a hand-built `GlobalObject` carrying just those two real
        // fields is exactly as valid a `bind()` target as a
        // registry-supplied one, same shortcut `native_host.rs::set_param`
        // takes.
        let global = GlobalObject {
            id: node_id,
            permissions: PermissionFlags::empty(),
            type_: ObjectType::Node,
            version: 0,
            props: None::<PropertiesBox>,
        };

        let _lock = self._thread_loop.lock();
        let node: pw::node::Node = self
            .registry
            .bind(&global)
            .map_err(|_| BackendError::Message(format!("failed to bind node {node_id}")))?;
        node.set_param(ParamType::from_raw(spa_sys::SPA_PARAM_Props), 0, pod);
        drop(node);

        Ok(())
    }

    /// Reads `node_id`'s current `Props` param via `Node::enum_params` +
    /// Pod deserialization (#424) — the read-side counterpart `set_props`
    /// didn't need, since pushing a param is fire-and-forget but reading one
    /// back is a genuine async round trip: `enum_params` doesn't return the
    /// value directly, it triggers a `param` event on a listener sometime
    /// later, once the server answers. This binds a `Node`, registers a
    /// one-shot listener that forwards the first `Props` param it sees down
    /// an `mpsc` channel, issues `enum_params`, then — critically — releases
    /// `thread_loop.lock()` *before* waiting on that channel: the callback
    /// only ever runs on the thread loop's own internal dispatch thread, so
    /// holding this process's lock while waiting for it would deadlock
    /// (nothing would ever run the loop far enough to invoke the callback).
    /// The node/listener are bound and dropped each under their own lock
    /// acquisition instead of one continuous critical section spanning the
    /// wait — `native_host.rs::set_param`'s "keep bind/use/drop under one
    /// lock" rule is about not letting a proxy outlive the lock *while some
    /// other thread might be touching the loop*, not about never unlocking
    /// in between; an async wait for a callback the loop's own thread must
    /// run is exactly the case where releasing the lock in between is
    /// required, not just permitted.
    fn query_props(&self, node_id: u32) -> Result<Value, BackendError> {
        let global = GlobalObject {
            id: node_id,
            permissions: PermissionFlags::empty(),
            type_: ObjectType::Node,
            version: 0,
            props: None::<PropertiesBox>,
        };
        let (tx, rx) = mpsc::channel::<Value>();

        let (node, listener) = {
            let _lock = self._thread_loop.lock();
            let node: pw::node::Node = self
                .registry
                .bind(&global)
                .map_err(|_| BackendError::Message(format!("failed to bind node {node_id}")))?;
            let listener = node
                .add_listener_local()
                .param(move |_seq, param_type, _index, _next, pod| {
                    if param_type != ParamType::from_raw(spa_sys::SPA_PARAM_Props) {
                        return;
                    }
                    let Some(pod) = pod else { return };
                    if let Ok((_, value)) =
                        PodDeserializer::deserialize_from::<Value>(pod.as_bytes())
                    {
                        let _ = tx.send(value);
                    }
                })
                .register();
            node.enum_params(
                0,
                Some(ParamType::from_raw(spa_sys::SPA_PARAM_Props)),
                0,
                u32::MAX,
            );
            (node, listener)
        };

        let result = rx.recv_timeout(PROPS_QUERY_TIMEOUT);

        {
            let _lock = self._thread_loop.lock();
            drop(listener);
            drop(node);
        }

        result.map_err(|_| {
            BackendError::Message(format!(
                "timed out waiting for Props param from node {node_id}"
            ))
        })
    }
}

/// Extracts `SPA_PROP_channelVolumes`'s first channel from a deserialized
/// `Props` `Value::Object`, converted back through the inverse of
/// [`linear_amplitude`] — `None` if this object isn't a `Props` param, or
/// doesn't carry a `channelVolumes` property at all (both real, if unlikely,
/// possibilities from a raw `enum_params` response, not treated as an error).
fn channel_volume_percent_from_props(value: &Value) -> Option<u8> {
    let Value::Object(object) = value else {
        return None;
    };
    object
        .properties
        .iter()
        .find(|property| property.key == spa_sys::SPA_PROP_channelVolumes)
        .and_then(|property| {
            let Value::ValueArray(ValueArray::Float(volumes)) = &property.value else {
                return None;
            };
            let linear = *volumes.first()?;
            Some((linear.max(0.0).cbrt() * 100.0).round().clamp(0.0, 100.0) as u8)
        })
}

/// Extracts `SPA_PROP_mute` from a deserialized `Props` `Value::Object`.
fn mute_state_from_props(value: &Value) -> Option<bool> {
    let Value::Object(object) = value else {
        return None;
    };
    object
        .properties
        .iter()
        .find(|property| property.key == spa_sys::SPA_PROP_mute)
        .and_then(|property| {
            let Value::Bool(muted) = property.value else {
                return None;
            };
            Some(muted)
        })
}

static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();

fn connection() -> Option<&'static Connection> {
    CONNECTION.get_or_init(Connection::start).as_ref()
}

fn node_name(global: &pw::registry::GlobalObject<&DictRef>) -> Option<String> {
    global.props?.get("node.name").map(|name| name.to_string())
}

fn run_assembler(
    rx: mpsc::Receiver<(u32, String)>,
    node_ids: Arc<Mutex<HashMap<String, u32>>>,
    ready_tx: mpsc::Sender<()>,
) {
    let mut ready_tx = Some(ready_tx);
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok((id, name)) => {
                node_ids
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(name, id);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if let Some(tx) = ready_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Cubic percent-to-linear-amplitude conversion — see this module's
/// top-level doc comment for why this specific curve, not a plain linear
/// `percent / 100.0`, is required to match `pactl`'s existing loudness for
/// the same percentage.
fn linear_amplitude(percent: u8) -> f32 {
    let fraction = percent.min(100) as f32 / 100.0;
    fraction.powf(3.0)
}

struct ChannelVolumesProp(Vec<f32>);

impl PodSerialize for ChannelVolumesProp {
    fn serialize<O: std::io::Write + std::io::Seek>(
        &self,
        serializer: PodSerializer<O>,
    ) -> Result<SerializeSuccess<O>, pw::spa::pod::serialize::GenError> {
        let mut obj = serializer
            .serialize_object(spa_sys::SPA_TYPE_OBJECT_Props, spa_sys::SPA_PARAM_Props)?;
        obj.serialize_property(
            spa_sys::SPA_PROP_channelVolumes,
            &self.0[..],
            PropertyFlags::empty(),
        )?;
        obj.end()
    }
}

struct MuteProp(bool);

impl PodSerialize for MuteProp {
    fn serialize<O: std::io::Write + std::io::Seek>(
        &self,
        serializer: PodSerializer<O>,
    ) -> Result<SerializeSuccess<O>, pw::spa::pod::serialize::GenError> {
        let mut obj = serializer
            .serialize_object(spa_sys::SPA_TYPE_OBJECT_Props, spa_sys::SPA_PARAM_Props)?;
        obj.serialize_property(spa_sys::SPA_PROP_mute, &self.0, PropertyFlags::empty())?;
        obj.end()
    }
}

/// Sets `primary_system_name`'s channel volumes to `percent`, then — if
/// `monitor_system_name` is given (the `{sink}.monitor` source PulseAudio
/// compat exposes for a virtual sink, mirrored by `pactl/mixer.rs`'s own
/// `uses_monitor_fan_out`/`monitor_source_name`) — the monitor's too, so a
/// virtual device's fan-out level tracks its primary level exactly like the
/// CLI path already keeps them in lockstep.
///
/// `None` (fall back to the `pactl` CLI path entirely) unless the native
/// connection is up *and* every node this call needs is already indexed —
/// resolving both ids up front rather than applying the primary natively
/// and silently falling back only for the monitor leg keeps this call
/// either fully native or fully CLI, never a native/CLI hybrid for a single
/// logical volume change.
pub fn set_device_volume(
    primary_system_name: &str,
    percent: u8,
    channels: u32,
    monitor_system_name: Option<&str>,
) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let primary_id = conn.node_id(primary_system_name)?;
    let monitor_id = match monitor_system_name {
        Some(name) => Some(conn.node_id(name)?),
        None => None,
    };

    let volumes = vec![linear_amplitude(percent); channels.max(1) as usize];
    let mut result = conn.set_props(primary_id, &ChannelVolumesProp(volumes.clone()));
    if result.is_ok() {
        if let Some(monitor_id) = monitor_id {
            result = conn.set_props(monitor_id, &ChannelVolumesProp(volumes));
        }
    }
    Some(result)
}

/// Mute counterpart to [`set_device_volume`] — same resolve-both-ids-or-fall-back
/// contract.
pub fn set_device_mute(
    primary_system_name: &str,
    muted: bool,
    monitor_system_name: Option<&str>,
) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let primary_id = conn.node_id(primary_system_name)?;
    let monitor_id = match monitor_system_name {
        Some(name) => Some(conn.node_id(name)?),
        None => None,
    };

    let mut result = conn.set_props(primary_id, &MuteProp(muted));
    if result.is_ok() {
        if let Some(monitor_id) = monitor_id {
            result = conn.set_props(monitor_id, &MuteProp(muted));
        }
    }
    Some(result)
}

/// Native equivalent of `pactl/mixer.rs::sink_volume_percent` (#424) — reads
/// `system_name`'s current channel volume back via `Node::enum_params` +
/// Pod deserialization rather than `pactl list sinks` text parsing. `None`
/// if the connection isn't up or the node isn't indexed (fall back to the
/// CLI); `Some(Ok(None))` if the query succeeded but the node's `Props`
/// param carried no `channelVolumes` (matches `sink_volume_percent`'s own
/// `Ok(None)` for "not found in the listing").
pub fn channel_volume_percent(system_name: &str) -> Option<Result<Option<u8>, BackendError>> {
    let conn = connection()?;
    let node_id = conn.node_id(system_name)?;
    Some(
        conn.query_props(node_id)
            .map(|value| channel_volume_percent_from_props(&value)),
    )
}

/// Native equivalent of `pactl/mixer.rs::sink_mute_state`.
pub fn mute_state(system_name: &str) -> Option<Result<Option<bool>, BackendError>> {
    let conn = connection()?;
    let node_id = conn.node_id(system_name)?;
    Some(
        conn.query_props(node_id)
            .map(|value| mute_state_from_props(&value)),
    )
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. Cross-checks this
    //! module's native writes against `pactl/mixer.rs::sink_volume_percent`/
    //! `sink_mute_state` — a completely independent pipeline (a fresh
    //! `pactl list sinks` shell + text parse), specifically to catch a
    //! cubic-curve mismatch the way no in-process assertion against this
    //! module's own state could: if `linear_amplitude` used a plain
    //! `percent / 100.0` instead of the cubic curve, this test would still
    //! pass an assertion against values *this module* computed, but would
    //! fail here, since `pactl`'s own read-back derives from the real
    //! server-side channel volume, converted back through the same cubic
    //! curve `pactl set-sink-volume` itself uses.
    use super::*;
    use crate::backend::linux::live::LinuxPipeWireBackend;
    use crate::backend::linux::pactl::{sink_mute_state, sink_volume_percent};
    use crate::backend::AudioBackend;
    use std::thread;
    use std::time::Duration;

    fn wait_until_indexed(name: &str) {
        let conn =
            connection().expect("native mixer connection should start against a real session");
        for _ in 0..50 {
            if conn.node_id(name).is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("timed out waiting for {name:?} to appear in the native mixer index");
    }

    #[test]
    #[ignore]
    fn sets_volume_and_mute_natively_matching_pactls_own_readback() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend
            .create_virtual_output("Pipe Deck Native Mixer Test", false)
            .expect("create disposable device");

        wait_until_indexed(&device.system_name);

        let volume_result = set_device_volume(&device.system_name, 42, 2, None);
        assert!(
            volume_result.is_some(),
            "expected the native path to run, not fall back to the CLI"
        );
        volume_result
            .unwrap()
            .expect("native volume set should succeed");

        let readback = (0..20).find_map(|_| {
            let percent = sink_volume_percent(&device.system_name).ok().flatten();
            if percent.is_some() {
                return percent;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        assert_eq!(
            readback,
            Some(42),
            "pactl's own readback should agree with the native 42% write"
        );

        let mute_result = set_device_mute(&device.system_name, true, None);
        assert!(mute_result.is_some());
        mute_result
            .unwrap()
            .expect("native mute set should succeed");

        let muted = (0..20).find_map(|_| {
            let state = sink_mute_state(&device.system_name).ok().flatten();
            if state.is_some() {
                return state;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        assert_eq!(
            muted,
            Some(true),
            "pactl's own readback should agree with the native mute"
        );

        let _ = backend.remove_virtual_device(&device.system_name);
    }

    #[test]
    #[ignore]
    fn reads_volume_and_mute_natively_matching_pactls_own_readback() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend
            .create_virtual_output("Pipe Deck Native Mixer Read Test", false)
            .expect("create disposable device");

        wait_until_indexed(&device.system_name);

        // Write via the CLI path directly (not this module's own writer) so
        // the read side is verified against a value it had no hand in
        // producing — a bug that made the read just echo back whatever this
        // module itself last wrote wouldn't be caught by round-tripping a
        // native write through a native read.
        crate::backend::linux::pactl::run_pactl(&["set-sink-volume", &device.system_name, "77%"])
            .expect("pactl set-sink-volume");
        crate::backend::linux::pactl::run_pactl(&["set-sink-mute", &device.system_name, "1"])
            .expect("pactl set-sink-mute");

        let volume = (0..20).find_map(|_| {
            let result = channel_volume_percent(&device.system_name)?;
            let percent = result.expect("native volume read should succeed");
            if percent.is_some() {
                return percent;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        assert_eq!(
            volume,
            Some(77),
            "expected the native read to agree with the value pactl itself set"
        );

        let muted = (0..20).find_map(|_| {
            let result = mute_state(&device.system_name)?;
            let state = result.expect("native mute read should succeed");
            if state.is_some() {
                return state;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        assert_eq!(
            muted,
            Some(true),
            "expected the native mute read to agree with the value pactl itself set"
        );

        let _ = backend.remove_virtual_device(&device.system_name);
    }
}
