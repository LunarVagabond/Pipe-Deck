//! Native, restart-free effects transport (issue #148, cutover to the
//! unconditional default in #149) — originated as the #141 spike
//! (`docs/architecture/Decisions.md` PD-027), then promoted into this real
//! module. Loads `libpipewire-module-filter-chain` directly into the live,
//! real `pipewire.service` session via `pw_context_load_module`, instead of
//! writing a conf.d drop-in and restarting a separate `filter-chain.service`
//! unit — that restart-based mechanism no longer exists in this codebase.
//!
//! ## Lifecycle
//!
//! One process-wide `ThreadLoopRc`/`ContextRc` pair, created once on first
//! use and held for the life of the process. The thread loop is started
//! immediately and runs continuously on its own dedicated OS thread for the
//! rest of the process's life — this is what actually lets a loaded
//! filter-chain instance participate in the real-time graph at all (issue
//! #303): a plain `pw_main_loop`, only manually `iterate()`d in short bursts
//! from whichever thread happens to call into this module, is never
//! listening when the driver wants to schedule this client's owned streams
//! the rest of the time, so anything it hosts sits permanently suspended
//! (confirmed live via `pw-top`: `QUANT=0`/state `S` indefinitely, even with
//! a correctly-wired, actively-playing upstream). Every PipeWire call below
//! is wrapped in `thread_loop.lock()`/drop to unlock — required whenever
//! touching an object associated with this loop, per `pw_thread_loop`'s own
//! contract — and released before any wait/sleep, since holding it would
//! stop the background thread from making progress during that wait.
//!
//! `pw::deinit()` is deliberately never called — the spike's own doc comment
//! found that calling it while `ContextRc`/the loop are still alive segfaults
//! on shutdown (their `Drop` impls call back into an already-torn-down
//! library). Rather than get per-call-site teardown ordering right, this
//! process simply never tears the library down and lets process exit
//! reclaim everything. See the PD-027 addendum in
//! `docs/architecture/Decisions.md`.
//!
//! ## Daemon ownership
//!
//! This module is only ever called from the daemon binary
//! (`daemon::ipc::server::dispatch`), never directly from the GUI (Tauri)
//! binary — the GUI talks to the daemon over `daemon::ipc::client` instead.
//! `daemon::mod.rs`'s systemd unit stays running (`Type=notify` +
//! `Restart=on-failure`) rather than restoring-then-exiting, so this
//! process-wide connection outlives any single GUI session. This only
//! covers users who've enabled restore-on-login (persistent daemon
//! installed/active) — a GUI-spawned, on-demand daemon for everyone else is
//! separate, not-yet-built work. See the PD-027 addendum in
//! `docs/architecture/Decisions.md`.
//!
//! A native in-memory connection doesn't survive the daemon process dying
//! the way a conf.d file did — if the daemon crashes or is restarted,
//! whatever chains were loaded are gone with it. `daemon::reconcile_live_effects_state`
//! re-derives and reloads persisted chains after a restart.

use crate::core::models::EffectChainConfig;
use crate::pipewire::{filter_chain, fx_validate};
use pipewire as pw;
use pipewire::spa;
use pipewire::spa::pod::serialize::{PodSerialize, PodSerializer, SerializeSuccess};
use pipewire::spa::pod::{Pod, PropertyFlags};
use pipewire::spa::sys as spa_sys;
use pipewire::sys as pw_sys;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::Cursor;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NativeHostError {
    #[error("pw_context_load_module returned NULL for {0} — module failed to load")]
    LoadFailed(String),
    #[error("module args for {0} contained a NUL byte")]
    InvalidArgs(String),
    #[error("no live PipeWire node found named {0}")]
    NodeNotFound(String),
    #[error("failed to bind a proxy for node {0}")]
    BindFailed(String),
    #[error("failed to build a Props pod: {0}")]
    PodBuildFailed(String),
}

/// Wraps `(control_name, value)` pairs as the `Struct` pod filter-chain's
/// `SPA_PROP_params` control expects — mirrors the *shape* `pw-cli set-param`
/// sends as JSON (`{ "params": [ "name", value, ... ] }`, the array form;
/// see `pipewire::pw_cli::set_params`'s doc comment for why the object/dict
/// form silently no-ops), built here as a real SPA pod instead of shelled
/// out through `pw-cli`.
struct EffectParams<'a>(&'a [(String, f64)]);

impl PodSerialize for EffectParams<'_> {
    fn serialize<O: std::io::Write + std::io::Seek>(
        &self,
        serializer: PodSerializer<O>,
    ) -> Result<SerializeSuccess<O>, spa::pod::serialize::GenError> {
        let mut struct_serializer = serializer.serialize_struct()?;
        for (name, value) in self.0 {
            struct_serializer.serialize_field(name.as_str())?;
            struct_serializer.serialize_field(value)?;
        }
        struct_serializer.end()
    }
}

/// The `Props` object carrying `EffectParams` under `SPA_PROP_params` — the
/// same object/property pair `pw-cli set-param <id> Props '...'` targets.
struct EffectProps<'a>(&'a [(String, f64)]);

impl PodSerialize for EffectProps<'_> {
    fn serialize<O: std::io::Write + std::io::Seek>(
        &self,
        serializer: PodSerializer<O>,
    ) -> Result<SerializeSuccess<O>, spa::pod::serialize::GenError> {
        let mut obj_serializer =
            serializer.serialize_object(spa_sys::SPA_TYPE_OBJECT_Props, spa_sys::SPA_PARAM_Props)?;
        obj_serializer.serialize_property(spa_sys::SPA_PROP_params, &EffectParams(self.0), PropertyFlags::empty())?;
        obj_serializer.end()
    }
}

/// Wraps the raw module pointer returned by `pw_context_load_module`. Only
/// ever touched while `NATIVE_HOST`'s mutex is held, so `Send` is safe even
/// though libpipewire's own thread-affinity rules would otherwise forbid
/// moving this across threads.
struct ModuleHandle(*mut pw_sys::pw_impl_module);
unsafe impl Send for ModuleHandle {}

struct NativeHost {
    thread_loop: pw::thread_loop::ThreadLoopRc,
    context: pw::context::ContextRc,
    // Held for the life of the process alongside `thread_loop`/`context`, for
    // the same reason: reused across every `set_param` call instead of
    // reconnecting (and re-doing the sync handshake) per slider tick.
    // `_core` must outlive `registry` — dropping it first would leave
    // `registry` pointing at a torn-down connection.
    registry: pw::registry::RegistryRc,
    _core: pw::core::CoreRc,
    // Kept alive for the connection's whole life so the `node_ids` index it
    // feeds keeps updating — dropping the listener early would silently
    // freeze the index at whatever it last saw.
    _node_listener: pw::registry::Listener,
    // `node.name -> id` (#430) — this daemon process's own equivalent of
    // `pw_registry.rs::NativeGraphWatcher`'s index (#411, GUI-process only,
    // since the daemon has no access to that connection — different OS
    // process). A plain `Arc<Mutex<..>>` rather than a field the outer
    // `NATIVE_HOST` mutex alone protects: the registry `global`/`global_remove`
    // callbacks run on the thread loop's own internal dispatch thread, which
    // must be able to update this without acquiring `NATIVE_HOST`'s mutex —
    // `load_chain`/`set_param` already hold that mutex while doing other
    // `pw` work, and a callback trying to re-acquire it recursively would
    // risk a deadlock this independent lock avoids entirely.
    node_ids: Arc<Mutex<HashMap<String, u32>>>,
    loaded: HashMap<String, ModuleHandle>,
}

// SAFETY: `ThreadLoopRc`/`ContextRc`/`RegistryRc`/`CoreRc` are designed to be
// controlled from a thread other than the one the loop itself runs on (that
// is the entire point of `pw_thread_loop`) — every access to them here goes
// through `thread_loop.lock()` first, matching that API's own contract.
// `node_ids` is a plain `Arc<Mutex<..>>`, `Send`-safe regardless.
unsafe impl Send for NativeHost {}

static NATIVE_HOST: OnceLock<Mutex<NativeHost>> = OnceLock::new();

fn host() -> &'static Mutex<NativeHost> {
    NATIVE_HOST.get_or_init(|| {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);
        // SAFETY: `pw::init()` has just been called above (exactly once,
        // process-wide, via `PW_INIT`).
        let thread_loop = unsafe { pw::thread_loop::ThreadLoopRc::new(Some("pipe-deck-native-host"), None) }
            .expect("failed to create PipeWire thread loop");
        thread_loop.start();

        let node_ids: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
        // `id -> name` isn't kept as a struct field — it only exists to let
        // the `global_remove` closure below know which `node_ids` entry a
        // dying id used to own, so it's captured by that closure alone
        // rather than exposed more broadly.
        let id_to_name: Arc<Mutex<HashMap<u32, String>>> = Arc::new(Mutex::new(HashMap::new()));

        let (context, core, registry, node_listener) = {
            let _lock = thread_loop.lock();
            let context = pw::context::ContextRc::new(&thread_loop, None).expect("failed to create PipeWire context");
            let core = context.connect_rc(None).expect("failed to connect to PipeWire core");
            let registry = core.get_registry_rc().expect("failed to get PipeWire registry");

            let add_node_ids = node_ids.clone();
            let add_id_to_name = id_to_name.clone();
            let remove_node_ids = node_ids.clone();
            let remove_id_to_name = id_to_name.clone();
            let node_listener = registry
                .add_listener_local()
                .global(move |global| {
                    if global.type_ != pw::types::ObjectType::Node {
                        return;
                    }
                    let Some(name) = global.props.and_then(|props| props.get("node.name")) else {
                        return;
                    };
                    let name = name.to_string();
                    add_node_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(name.clone(), global.id);
                    add_id_to_name.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(global.id, name);
                })
                .global_remove(move |id| {
                    let Some(name) =
                        remove_id_to_name.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id)
                    else {
                        return;
                    };
                    let mut node_ids = remove_node_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    // Only remove if this id is still on record for that
                    // name — guards against a rare remove-then-immediately-
                    // recreate-with-a-new-id race clobbering a just-inserted
                    // newer entry (same guard `pw_registry.rs::apply_event`
                    // and `pw_link_native.rs::Index::apply` both use).
                    if node_ids.get(&name) == Some(&id) {
                        node_ids.remove(&name);
                    }
                })
                .register();

            (context, core, registry, node_listener)
        };

        Mutex::new(NativeHost {
            thread_loop,
            context,
            registry,
            _core: core,
            _node_listener: node_listener,
            node_ids,
            loaded: HashMap::new(),
        })
    })
}

/// Resolves `node_name`'s live id via this connection's own `node_ids`
/// index (#430) — `None` on a miss, same "fall back to the shellout for
/// what's missing" contract #411 established for the GUI-process
/// equivalent, since a node created within the last instant may not have
/// reached the index yet.
fn find_live_node_id(guard: &NativeHost, node_name: &str) -> Option<u32> {
    guard.node_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(node_name).copied()
}

/// How long to sleep, lock released, per poll attempt while waiting for the
/// background thread to complete an async round trip (a registry `global`
/// event arriving, a just-loaded module's ports becoming visible to an
/// external `pw-link`/`pw-dump` query). Must never be called while holding
/// `thread_loop.lock()` — that would stop the very thread this is waiting on.
const SETTLE_INTERVAL: Duration = Duration::from_millis(50);
/// Matches the previous manually-pumped design's total wait budget
/// (20 x 50ms = 1s) — kept the same even though the continuously-running
/// thread loop should settle far faster in practice, since this only trades
/// off worst-case latency for a within-budget guess, not correctness.
const SETTLE_ATTEMPTS: u32 = 20;

fn settle() {
    std::thread::sleep(SETTLE_INTERVAL);
}

/// Forces `host()`'s one-time PipeWire connection setup (`pw::init()`,
/// spinning up the thread loop, connecting the context, fetching the
/// registry — each a real round trip with the server) to happen now, rather
/// than lazily inside whichever `load_chain`/`unload_chain`/`is_loaded`/
/// `set_param` call happens to be first. Meant to be called once at daemon
/// startup, before `ipc::server::run()` starts accepting requests: without
/// this, that one-time cost is paid inside the first real IPC request instead
/// — and a cold process (first-ever `pw::init()`, cold page cache, ...) can
/// take long enough that it blows through the client's `REQUEST_TIMEOUT`
/// (issue: reported as "native-effects daemon is unreachable: Resource
/// temporarily unavailable (os error 11)" on whichever effect a user happens
/// to add first after a fresh daemon start — not specific to any one effect
/// kind, since `reconcile_live_effects_state`/`reconcile_live_processing_nodes`
/// only touch `native_host` at all when there's something persisted to
/// reload, so a fresh profile with nothing yet configured never warms it up
/// on its own).
pub fn warm_up() {
    host();
}

/// Loads `config`'s filter chain onto `device_system_name`, swapping out
/// whatever chain (if any) is already loaded for it first (PD-020:
/// swap-by-identity, same node name takes over). `is_input` picks which of
/// `EffectChainConfig`'s two node-name/media-class templates to render
/// (`fx_validate::render_module_args` vs `render_module_args_capture`) —
/// `EffectChainConfig` carries no direction of its own. Returns the
/// downstream-linkable playback node name (`effect_output.*`/
/// `effect_input.*`, matching the restart-based path's shadow naming
/// exactly) — the playback side never auto-links, per the spike's findings.
pub fn load_chain(device_system_name: &str, is_input: bool, config: &EffectChainConfig) -> Result<String, NativeHostError> {
    if is_loaded(device_system_name) {
        unload_chain(device_system_name)?;
    }

    let args = if is_input {
        fx_validate::render_module_args_capture(device_system_name, config)
    } else {
        fx_validate::render_module_args(device_system_name, config)
    };
    let playback_name = if is_input {
        device_system_name.to_string()
    } else {
        filter_chain::effect_output_name_for_device(device_system_name)
    };

    let module_name_c = CString::new("libpipewire-module-filter-chain").expect("static string has no NUL");
    let args_c = CString::new(args).map_err(|_| NativeHostError::InvalidArgs(device_system_name.to_string()))?;

    let mut guard = host().lock().expect("native host mutex poisoned");
    let module_ptr = {
        let _lock = guard.thread_loop.lock();
        unsafe {
            pw_sys::pw_context_load_module(guard.context.as_raw_ptr(), module_name_c.as_ptr(), args_c.as_ptr(), std::ptr::null_mut())
        }
    };
    if module_ptr.is_null() {
        return Err(NativeHostError::LoadFailed(device_system_name.to_string()));
    }

    guard.loaded.insert(device_system_name.to_string(), ModuleHandle(module_ptr));
    let node_ids = guard.node_ids.clone();
    drop(guard);

    // Wait for the playback side's ports to actually be visible externally
    // (to `pw-link`/`pw-dump`, not just to this process) before returning —
    // callers like `virtual_mic_mix::relink_feeds_to` immediately try to
    // `pw-link` into the returned name, and a fixed short sleep here isn't
    // enough of a margin: the module registering with the server is an async
    // round trip whose latency isn't bounded by anything this process
    // controls. Same bounded budget as `set_param`'s lookup, just polling a
    // different condition (port visibility, not node existence). Checks this
    // connection's own `node_ids` index (#430) first, falling back to the
    // `pw-dump` shellout on a miss.
    for _ in 0..SETTLE_ATTEMPTS {
        let found = node_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).contains_key(&playback_name)
            || crate::pipewire::pw_cli::find_node_id_by_name(&playback_name).ok().flatten().is_some();
        if found {
            break;
        }
        settle();
    }

    Ok(playback_name)
}

/// Unloads a previously loaded chain. A no-op if nothing is loaded for
/// `device_system_name` — mirrors `revert_to_plain_device`'s tolerance of
/// being called on a device that's already plain.
pub fn unload_chain(device_system_name: &str) -> Result<(), NativeHostError> {
    let mut guard = host().lock().expect("native host mutex poisoned");
    let Some(handle) = guard.loaded.remove(device_system_name) else {
        return Ok(());
    };
    {
        let _lock = guard.thread_loop.lock();
        unsafe { pw_sys::pw_impl_module_destroy(handle.0) };
    }
    drop(guard);
    settle();
    Ok(())
}

/// Whether a chain is currently loaded for `device_system_name`.
pub fn is_loaded(device_system_name: &str) -> bool {
    host().lock().expect("native host mutex poisoned").loaded.contains_key(device_system_name)
}

/// Pushes a live `Props` param update — `(control_name, value)` pairs — to
/// the already-loaded filter-chain node named `device_system_name`, over
/// this process's own persistent PipeWire connection for the actual push.
///
/// Finds the target node's id via this connection's own `node_ids` index
/// first (#430), falling back to a synchronous `pw-dump` snapshot
/// (`pipewire::pw_cli::find_node_id_by_name`) on a miss. `node_ids` isn't a
/// one-off listener registered at call time — a *fresh* listener only ever
/// sees a `global` *announcement* event, which the server sends exactly
/// once, the first time each object becomes visible, so a listener
/// registered after that (as this function would if it made its own) never
/// sees it again — that's what made every EQ slider drag after the first
/// fail with "no live PipeWire node found" before #303's fix. `node_ids` is
/// instead registered once, when this connection is first created
/// (`host()`), and kept continuously updated for the connection's whole
/// life — so by the time any given `set_param` call runs, it's very likely
/// already seen the node's original announcement, whenever that was.
pub fn set_param(device_system_name: &str, params: &[(String, f64)]) -> Result<(), NativeHostError> {
    if params.is_empty() {
        return Ok(());
    }

    let guard = host().lock().expect("native host mutex poisoned");

    let mut id = None;
    for attempt in 0..SETTLE_ATTEMPTS {
        id = find_live_node_id(&guard, device_system_name)
            .or_else(|| crate::pipewire::pw_cli::find_node_id_by_name(device_system_name).ok().flatten());
        if id.is_some() || attempt + 1 == SETTLE_ATTEMPTS {
            break;
        }
        settle();
    }
    let Some(id) = id else {
        return Err(NativeHostError::NodeNotFound(device_system_name.to_string()));
    };

    // `bind()` only reads `id` and (via `type_.client_version()`) `type_` —
    // `permissions`/`version`/`props` are unused by it, so a hand-built
    // `GlobalObject` carrying just those two real fields (the target is
    // always a Node — the only kind `pw-dump`'s lookup above matches) is
    // exactly as valid a `bind()` target as a registry-supplied one.
    let global = pw::registry::GlobalObject {
        id,
        permissions: pw::permissions::PermissionFlags::empty(),
        type_: pw::types::ObjectType::Node,
        version: 0,
        props: None::<pw::properties::PropertiesBox>,
    };

    let mut bytes = Vec::new();
    PodSerializer::serialize(Cursor::new(&mut bytes), &EffectProps(params))
        .map_err(|error| NativeHostError::PodBuildFailed(format!("{error:?}")))?;
    let pod = Pod::from_bytes(&bytes)
        .ok_or_else(|| NativeHostError::PodBuildFailed("serialized pod bytes were malformed".into()))?;

    {
        let _lock = guard.thread_loop.lock();
        // `node`'s scope (bind, use, and — critically — `Drop`, which tears
        // down the proxy) is kept entirely inside this single lock guard.
        // Letting it outlive the lock (as an earlier version of this
        // function did, binding under one `lock()`/drop and only
        // `set_param`-ing under a second, separate one) meant `node`'s own
        // destructor ran with no lock held at all once the function
        // returned — PipeWire's own thread-safety checks caught this live
        // (`*** impl_ext_end_proxy called from wrong context, check thread
        // and locking`), since destroying a proxy is exactly the kind of
        // "touches an object associated with this loop" operation the lock
        // contract requires.
        let node: pw::node::Node =
            guard.registry.bind(&global).map_err(|_| NativeHostError::BindFailed(device_system_name.to_string()))?;
        node.set_param(spa::param::ParamType::from_raw(spa_sys::SPA_PARAM_Props), 0, pod);
        drop(node);
    }
    drop(guard);
    settle();

    Ok(())
}

/// Test-only accessor for `host()`'s own `node_ids` index — lets
/// `live_tests` confirm the index actually resolved a node natively rather
/// than every call quietly falling through to the `pw_cli` shellout, which a
/// pass/fail assertion on `load_chain`/`set_param`'s own return value alone
/// couldn't distinguish (both paths produce the same success either way).
#[cfg(test)]
fn is_indexed(node_name: &str) -> bool {
    find_live_node_id(&host().lock().expect("native host mutex poisoned"), node_name).is_some()
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. Runs in-process (unlike
    //! `daemon::ipc::client::live_tests`, which spawns a real daemon
    //! subprocess and talks over IPC) so it can inspect `host()`'s own
    //! `node_ids` index directly via `is_indexed` — proof the native lookup
    //! actually resolved something, not just that `load_chain`/`set_param`
    //! happened to succeed via the `pw_cli` fallback either way.
    use super::*;
    use crate::core::models::EffectStage;

    #[test]
    #[ignore]
    fn load_chain_and_set_param_resolve_the_node_via_the_native_index() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let device_system_name = "pipe-deck-native-host-index-test";
        let cleanup = || {
            let _ = unload_chain(device_system_name);
        };

        let config = EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 0,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let playback_name = match load_chain(device_system_name, false, &config) {
            Ok(name) => name,
            Err(error) => {
                cleanup();
                panic!("load_chain failed: {error}");
            }
        };

        if !is_indexed(&playback_name) {
            cleanup();
            panic!("expected the native node_ids index to have resolved {playback_name:?} after load_chain");
        }

        // Waits well past the narrow "just announced" window (`settle`'s own
        // budget already elapsed once inside `load_chain`), matching
        // `daemon::ipc::client::live_tests::set_param_finds_a_node_created_well_before_the_call`'s
        // own regression rationale — a persistent index, unlike a one-off
        // listener, should resolve this exactly as well seconds later as it
        // did immediately after creation.
        std::thread::sleep(Duration::from_secs(2));

        if !is_indexed(device_system_name) {
            cleanup();
            panic!("expected the native node_ids index to still have {device_system_name:?} well after creation");
        }

        if let Err(error) = set_param(device_system_name, &[("eq_bass:Gain".to_string(), 3.0)]) {
            cleanup();
            panic!("set_param failed well after node creation: {error}");
        }

        cleanup();
    }
}
