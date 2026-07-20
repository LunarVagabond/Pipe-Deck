//! Native, restart-free effects transport (issue #148) — promotes the #141
//! spike (`examples/filter_chain_spike.rs`) into a real module. Loads
//! `libpipewire-module-filter-chain` directly into the live, real
//! `pipewire.service` session via `pw_context_load_module`, instead of
//! writing a conf.d drop-in and restarting the separate `filter-chain.service`
//! unit (`pipewire::pipewire_restart`).
//!
//! Opt-in only: requires both the `native-effects` Cargo feature (this whole
//! module is compiled out otherwise, see `pipewire::mod`) and
//! `PIPE_DECK_NATIVE_EFFECTS=1` at runtime (see
//! `backend::linux::live::LinuxPipeWireBackend::effect_chain_capabilities`).
//! The restart-based path remains the unconditional default for everyone
//! else.
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
//! is the landing spot for re-deriving/reloading persisted chains after a
//! restart, but is currently an empty stub — recovery is not yet
//! implemented.
// TODO(#148 follow-up): recovery from daemon restart — nothing recovers yet,
// see `daemon::reconcile_live_effects_state`.

use crate::core::models::EffectChainConfig;
use crate::pipewire::{filter_chain, fx_validate};
use pipewire as pw;
use pipewire::sys as pw_sys;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NativeHostError {
    #[error("pw_context_load_module returned NULL for {0} — module failed to load")]
    LoadFailed(String),
    #[error("module args for {0} contained a NUL byte")]
    InvalidArgs(String),
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
        Mutex::new(NativeHost {
            mainloop,
            context,
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
