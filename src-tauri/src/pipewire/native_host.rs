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
//! One process-wide `MainLoopRc`/`ContextRc` pair, created once on first use
//! and held for the life of the process. `pw::deinit()` is deliberately
//! never called — the spike's own doc comment found that calling it while
//! `ContextRc`/`MainLoopRc` are still alive segfaults on shutdown (their
//! `Drop` impls call back into an already-torn-down library). Rather than
//! get per-call-site teardown ordering right, this process simply never
//! tears the library down and lets process exit reclaim everything. See the
//! PD-027 addendum in `docs/architecture/Decisions.md`.
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
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};
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
    mainloop: pw::main_loop::MainLoopRc,
    context: pw::context::ContextRc,
    // Held for the life of the process alongside `mainloop`/`context`, for
    // the same reason: reused across every `set_param` call instead of
    // reconnecting (and re-doing the sync handshake) per slider tick.
    // `_core` must outlive `registry` — dropping it first would leave
    // `registry` pointing at a torn-down connection.
    registry: pw::registry::RegistryRc,
    _core: pw::core::CoreRc,
    loaded: HashMap<String, ModuleHandle>,
}

// SAFETY: `MainLoopRc`/`ContextRc` are only ever touched from inside
// `host()`'s mutex, one caller at a time — never concurrently, and never
// relied upon to stay pinned to a single OS thread.
unsafe impl Send for NativeHost {}

static NATIVE_HOST: OnceLock<Mutex<NativeHost>> = OnceLock::new();

fn host() -> &'static Mutex<NativeHost> {
    NATIVE_HOST.get_or_init(|| {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);
        let mainloop = pw::main_loop::MainLoopRc::new(None).expect("failed to create PipeWire main loop");
        let context = pw::context::ContextRc::new(&mainloop, None).expect("failed to create PipeWire context");
        let core = context.connect_rc(None).expect("failed to connect to PipeWire core");
        let registry = core.get_registry_rc().expect("failed to get PipeWire registry");
        pump(mainloop.loop_());
        Mutex::new(NativeHost {
            mainloop,
            context,
            registry,
            _core: core,
            loaded: HashMap::new(),
        })
    })
}

/// Pumps the main loop briefly so a just-issued load/unload's async
/// node/port setup actually completes before the caller relies on it having
/// happened — mirrors the spike's own pump loop (~20x50ms).
fn pump(loop_: &pw::loop_::Loop) {
    for _ in 0..20 {
        loop_.iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
    }
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
    let module_ptr = unsafe {
        pw_sys::pw_context_load_module(guard.context.as_raw_ptr(), module_name_c.as_ptr(), args_c.as_ptr(), std::ptr::null_mut())
    };
    if module_ptr.is_null() {
        return Err(NativeHostError::LoadFailed(device_system_name.to_string()));
    }

    pump(guard.mainloop.loop_());
    guard.loaded.insert(device_system_name.to_string(), ModuleHandle(module_ptr));

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
    unsafe { pw_sys::pw_impl_module_destroy(handle.0) };
    pump(guard.mainloop.loop_());
    Ok(())
}

/// Whether a chain is currently loaded for `device_system_name`.
pub fn is_loaded(device_system_name: &str) -> bool {
    host().lock().expect("native host mutex poisoned").loaded.contains_key(device_system_name)
}

/// Pushes a live `Props` param update — `(control_name, value)` pairs — to
/// the already-loaded filter-chain node named `device_system_name`, over
/// this process's own persistent PipeWire connection rather than shelling
/// out to `pw-dump`+`pw-cli set-param` per call (`pipewire::pw_cli`, still
/// used by the device-attached EQ path, PD-020). Async, fire-and-forget on
/// the node's `set_param` method itself (the native protocol gives no other
/// per-call ack — see PipeWire's own native-protocol docs), but the registry
/// lookup below only returns once the target node is actually found (or the
/// scan times out), which is the meaningful confirmation that the push has
/// somewhere real to land.
pub fn set_param(device_system_name: &str, params: &[(String, f64)]) -> Result<(), NativeHostError> {
    if params.is_empty() {
        return Ok(());
    }

    let guard = host().lock().expect("native host mutex poisoned");

    let found: Rc<RefCell<Option<pw::registry::GlobalObject<pw::properties::PropertiesBox>>>> =
        Rc::new(RefCell::new(None));
    let target_name = device_system_name.to_string();
    let found_for_listener = Rc::clone(&found);
    let listener = guard
        .registry
        .add_listener_local()
        .global(move |global| {
            if found_for_listener.borrow().is_some() {
                return;
            }
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }
            let Some(props) = global.props.as_ref() else {
                return;
            };
            if props.get("node.name") == Some(target_name.as_str()) {
                *found_for_listener.borrow_mut() = Some(global.to_owned());
            }
        })
        .register();

    pump(guard.mainloop.loop_());
    drop(listener);

    let Some(global) = found.borrow_mut().take() else {
        return Err(NativeHostError::NodeNotFound(device_system_name.to_string()));
    };

    let node: pw::node::Node = guard
        .registry
        .bind(&global)
        .map_err(|_| NativeHostError::BindFailed(device_system_name.to_string()))?;

    let mut bytes = Vec::new();
    PodSerializer::serialize(Cursor::new(&mut bytes), &EffectProps(params))
        .map_err(|error| NativeHostError::PodBuildFailed(format!("{error:?}")))?;
    let pod = Pod::from_bytes(&bytes)
        .ok_or_else(|| NativeHostError::PodBuildFailed("serialized pod bytes were malformed".into()))?;

    node.set_param(spa::param::ParamType::from_raw(spa_sys::SPA_PARAM_Props), 0, pod);

    pump(guard.mainloop.loop_());

    Ok(())
}
