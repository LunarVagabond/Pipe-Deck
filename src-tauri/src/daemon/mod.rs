use crate::config::ConfigStore;
use crate::core::models::DaemonStatus;
use crate::core::restore::{self, RestoreError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

pub mod ipc;

const SERVICE_NAME: &str = "pipe-deck-daemon.service";
/// Bumped whenever `packaging/pipe-deck-daemon.service` changes in a way that
/// requires reinstalling an already-installed unit rather than just
/// overwriting the file in place (e.g. the `Type=oneshot` -> `Type=notify`
/// change for issue #148 — systemd doesn't pick that up from a plain
/// `daemon-reload` on a unit that's already active under the old type).
/// Matches the `# pipe-deck-daemon-unit-version: N` comment at the top of the
/// bundled unit file.
const CURRENT_UNIT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStateFile {
    pub pid: u32,
    pub last_run: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub devices_restored: u32,
}

pub fn state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(path).join("pipe-deck");
    }
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".local/state/pipe-deck"))
        .unwrap_or_else(|_| PathBuf::from(".pipe-deck-state"))
}

pub fn state_file_path() -> PathBuf {
    state_dir().join("daemon.json")
}

pub fn user_systemd_dir() -> PathBuf {
    if let Ok(path) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("systemd/user");
    }
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/systemd/user"))
        .unwrap_or_else(|_| PathBuf::from(".config/systemd/user"))
}

pub fn write_status(pid: u32, last_run: &str, last_error: Option<&str>, devices_restored: u32) {
    let state = DaemonStateFile {
        pid,
        last_run: last_run.to_string(),
        last_error: last_error.map(str::to_string),
        devices_restored,
    };
    let path = state_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(path, contents);
    }
}

pub fn read_status() -> Option<DaemonStateFile> {
    let contents = fs::read_to_string(state_file_path()).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn run() -> i32 {
    let pid = std::process::id();
    let started = Utc::now().to_rfc3339();
    let store = ConfigStore::new();

    if let Err(error) = store.ensure_layout() {
        write_status(pid, &started, Some(&error.to_string()), 0);
        return 0;
    }

    let config = match store.load_config() {
        Ok(config) => config,
        Err(error) => {
            write_status(pid, &started, Some(&error.to_string()), 0);
            return 0;
        }
    };

    if !config.preferences.background_restore {
        return 0;
    }

    let backend = crate::backend::create_backend();
    let mut last_error = None;
    let mut devices_restored = 0u32;

    for attempt in 0..5 {
        match restore::restore_session(backend.as_ref()) {
            Ok(result) => {
                devices_restored =
                    (result.created.len() + result.adopted.len()) as u32;
                if result.errors.is_empty() {
                    if let Err(error) = restore::apply_persisted_routes(backend.as_ref()) {
                        last_error = Some(error.to_string());
                    } else {
                        last_error = None;
                        break;
                    }
                } else {
                    last_error = Some(result.errors.join("; "));
                }
            }
            Err(RestoreError::Config(message) | RestoreError::Adapter(message)) => {
                last_error = Some(message);
            }
        }

        if attempt < 4 {
            thread::sleep(Duration::from_secs(2));
        }
    }

    write_status(
        pid,
        &started,
        last_error.as_deref(),
        devices_restored,
    );

    // Native-effects hosting (issue #148): the daemon should come up for
    // effects hosting regardless of whether restore succeeded — restore
    // failures don't block a user from attaching a live effect chain. This
    // only returns if the socket bind itself fails.
    serve_native_effects();
    0
}

/// Entry point for a GUI-spawned, ephemeral daemon instance (issue #148) —
/// used when the user hasn't enabled restore-on-login, so no persistent
/// systemd-managed daemon is already running, but native effects transport
/// is still wanted for this GUI session. Selected by the
/// `PIPE_DECK_DAEMON_EPHEMERAL=1` env var in `bin/pipe-deck-daemon.rs`,
/// entirely separate from the `background_restore` config flag `run()`
/// checks — this process was spawned directly by the GUI as a plain child,
/// not installed/enabled via systemd, and has nothing to do with that
/// setting.
///
/// Skips the virtual-device/routing restore-session retry loop entirely:
/// unlike a fresh login, the GUI is already up and already has its own
/// up-to-date view of devices/routing, so there's nothing stale to restore
/// here — this process exists purely to host native effects for as long as
/// the GUI that spawned it is alive. Its own crash-safety (getting killed if
/// the GUI dies, including a crash) is the spawning side's job
/// (`ensure_ephemeral_daemon`'s `PR_SET_PDEATHSIG`), not this function's.
pub fn run_ephemeral() -> i32 {
    serve_native_effects();
    0
}

/// Enters the daemon's long-running phase: notifies `systemd` (`Type=notify`)
/// that startup is complete, then blocks forever serving native-effects IPC
/// requests (`ipc::server::run`).
fn serve_native_effects() {
    reconcile_live_effects_state();
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);
    let _ = ipc::server::run();
}

/// Re-derives live-effects state after a daemon (re)start — a native
/// in-memory connection doesn't survive the daemon dying the way a conf.d
/// file did, so whatever chains were loaded before a crash/restart are
/// already gone (they belonged to the dead connection) by the time this
/// runs. Reloads anything that's persisted as active but not currently
/// loaded, straight through `native_host`, before the socket starts
/// accepting requests.
///
/// Deliberately does **not** re-establish downstream routing (fan-out
/// targets, mic-mix feeders) for the reloaded `effect_output.*`/
/// `effect_input.*` nodes — that's `backend::linux::graph_routing`'s job,
/// and it already runs generically on every GUI graph refresh, keyed off
/// persisted routing intent, with no idea (or need to know) how a node came
/// to exist. So a reloaded chain's audio processing comes back immediately;
/// its downstream links come back on the GUI's next refresh, not
/// instantaneously. See the PD-027 addendum.
///
/// No `CoreEngine`/`RuntimeGraph` involved on purpose — same
/// `&dyn AudioBackend` + `ConfigStore` shape `core::restore` already uses
/// daemon-side for virtual-device/routing restore. Resolving "which
/// persisted chain belongs to which live device" doesn't need graph state:
/// for a `pipe-deck-*` virtual device, `Device.id` is always exactly
/// `format!("virtual-{}", system_name.trim_start_matches("pipe-deck-"))`
/// (`backend::linux::virtual_devices`), and `list_virtual_devices()` already
/// returns that `device_id` paired with `system_name`/`direction` directly.
fn reconcile_live_effects_state() {
    let Ok(chains) = ConfigStore::new().effect_chains() else {
        return;
    };
    if chains.is_empty() {
        return;
    }

    let backend = crate::backend::create_backend();
    for info in backend.list_virtual_devices() {
        let Some(config) = chains.get(&info.device_id) else {
            continue;
        };
        if !config.is_active() || crate::pipewire::native_host::is_loaded(&info.system_name) {
            continue;
        }

        let is_input = info.direction == crate::core::models::DeviceDirection::Input;
        let _ = crate::pipewire::native_host::load_chain(&info.system_name, is_input, config);
    }
}

/// GUI-managed handle to a spawned ephemeral daemon child (issue #148),
/// stored as Tauri state so the run-event loop can kill it on `RunEvent::Exit`
/// — belt-and-suspenders for a clean quit, on top of `PR_SET_PDEATHSIG`'s
/// kernel-level guarantee for the crash case (see `ensure_ephemeral_daemon`).
/// `None` when nothing was spawned (either a persistent daemon already
/// answered a ping or spawning failed) — `kill_ephemeral_daemon` is a safe
/// no-op either way.
pub struct EphemeralDaemonHandle(pub std::sync::Mutex<Option<std::process::Child>>);

/// Spawns a lightweight instance of the daemon binary as a plain child
/// process of the GUI, for users who haven't enabled restore-on-login (so no
/// persistent systemd daemon is already running) but still want native
/// effects transport for this session. Ping-first: if anything (typically
/// the persistent daemon) already answers the socket, does nothing — the
/// GUI never needs to know or care which one it's actually talking to.
///
/// Crash-safety: `PR_SET_PDEATHSIG` makes the kernel send `SIGKILL` to the
/// child the moment *this* process dies, for any reason — a clean quit, a
/// crash, an external `kill -9`, an OOM-kill. This is the load-bearing
/// guarantee; nothing that depends on Rust code actually running during
/// shutdown (a `Drop` impl, a `RunEvent::Exit` handler) can cover a crash,
/// since no code runs in a process that's already dead. `RunEvent::Exit`
/// (wired up in `lib.rs`) still kills the child explicitly too, but only as
/// a faster/cleaner path for the ordinary quit case — `PR_SET_PDEATHSIG` is
/// what makes the *lingering-process* guarantee actually hold.
///
/// Deliberately does not unload any effect chains before the child dies —
/// same as the persistent daemon, its native PipeWire connection dies with
/// the process either way, and for a user who hasn't opted into persistence,
/// effects *not* surviving the app closing is the intended behavior, not a
/// gap (confirmed against the underlying product question this addresses).
pub fn ensure_ephemeral_daemon() -> Option<std::process::Child> {
    if ipc::client::NativeHostClient::ping() {
        return None;
    }

    let path = daemon_binary_path()?;
    let mut command = Command::new(path);
    command.env("PIPE_DECK_DAEMON_EPHEMERAL", "1");

    // SAFETY: the closure only calls `libc::prctl`, an async-signal-safe
    // syscall — the one operation `pre_exec` closures are documented to be
    // sound for (arbitrary Rust runtime/allocator use between fork and exec
    // is what's actually unsound here, and this closure does neither).
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            // FFI call with no preconditions beyond the signal number being
            // valid, which `libc::SIGKILL` guarantees — sound under the
            // same `pre_exec` safety contract as the outer block.
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command.spawn().ok()
}

/// Kills a previously spawned ephemeral daemon, if any. Safe to call even if
/// nothing was ever spawned (persistent daemon was already running, or
/// spawning failed) — a no-op in either case.
pub fn kill_ephemeral_daemon(handle: &EphemeralDaemonHandle) {
    if let Ok(mut guard) = handle.0.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
        }
    }
}

pub fn daemon_binary_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PIPE_DECK_DAEMON_PATH") {
        return Some(PathBuf::from(path));
    }

    if let Ok(current) = std::env::current_exe() {
        let sibling = current
            .parent()
            .map(|dir| dir.join("pipe-deck-daemon"));
        if let Some(path) = sibling {
            if path.exists() {
                return Some(path);
            }
        }
    }

    for candidate in [
        PathBuf::from("/usr/bin/pipe-deck-daemon"),
        PathBuf::from("/app/bin/pipe-deck-daemon"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    find_in_path("pipe-deck-daemon")
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn bundled_service_unit() -> String {
    include_str!("../../packaging/pipe-deck-daemon.service").to_string()
}

/// Parses the `# pipe-deck-daemon-unit-version: N` marker from an installed
/// unit file's contents, if present. `None` covers both "file doesn't exist"
/// and "predates the marker entirely" (the original `Type=oneshot` unit) —
/// both cases need the same reinstall treatment below.
fn installed_unit_version(contents: &str) -> Option<u32> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("# pipe-deck-daemon-unit-version:"))
        .and_then(|value| value.trim().parse().ok())
}

pub fn install_user_service_unit() -> Result<(), String> {
    let daemon_path = daemon_binary_path()
        .ok_or_else(|| "pipe-deck-daemon binary not found".to_string())?;
    let unit_dir = user_systemd_dir();
    fs::create_dir_all(&unit_dir).map_err(|error| error.to_string())?;

    let unit_path = unit_dir.join(SERVICE_NAME);
    // The `Type=oneshot` -> `Type=notify` change (issue #148) isn't something
    // systemd picks up from a plain overwrite + `daemon-reload` on a unit
    // that's already active under the old type — stop the old unit first so
    // the new one starts clean under its new `Type=`.
    if let Ok(existing) = fs::read_to_string(&unit_path) {
        let needs_reinstall = installed_unit_version(&existing).is_none_or(|version| version < CURRENT_UNIT_VERSION);
        if needs_reinstall {
            let _ = run_systemctl(&["disable", "--now", SERVICE_NAME]);
        }
    }

    let unit = bundled_service_unit().replace(
        "ExecStart=/usr/bin/pipe-deck-daemon",
        &format!("ExecStart={}", daemon_path.display()),
    );
    fs::write(&unit_path, unit).map_err(|error| error.to_string())?;
    run_systemctl(&["daemon-reload"])?;
    Ok(())
}

pub fn enable_background_service() -> Result<(), String> {
    install_user_service_unit()?;
    run_systemctl(&["enable", "--now", SERVICE_NAME])?;
    ConfigStore::new()
        .set_background_restore(true)
        .map_err(|error| error.to_string())
}

pub fn disable_background_service() -> Result<(), String> {
    let _ = run_systemctl(&["disable", "--now", SERVICE_NAME]);
    ConfigStore::new()
        .set_background_restore(false)
        .map_err(|error| error.to_string())
}

/// Full teardown of the background-restore unit: disables/stops it (like
/// `disable_background_service`) and additionally deletes the unit file
/// itself, which that function deliberately leaves in place. Meant for
/// package uninstall/purge (`pipe-deck-cli cleanup`), not the Settings
/// toggle — a user turning background restore back on later should still
/// find a working unit without needing to reinstall the package.
/// Returns the removed unit path, or `None` if there was nothing to remove.
pub fn uninstall_user_service_unit() -> Result<Option<PathBuf>, String> {
    let unit_path = user_systemd_dir().join(SERVICE_NAME);
    if !unit_path.exists() {
        return Ok(None);
    }
    let _ = run_systemctl(&["disable", "--now", SERVICE_NAME]);
    fs::remove_file(&unit_path).map_err(|error| error.to_string())?;
    let _ = run_systemctl(&["daemon-reload"]);
    Ok(Some(unit_path))
}

pub fn get_status() -> DaemonStatus {
    let enabled = is_service_enabled();
    let running = is_service_running();
    let state = stale_state_filter(enabled, read_status());

    DaemonStatus {
        running,
        enabled,
        pid: state.as_ref().map(|value| value.pid),
        last_run: state.as_ref().map(|value| value.last_run.clone()),
        last_error: state.as_ref().and_then(|value| value.last_error.clone()),
        devices_restored: state.as_ref().map(|value| value.devices_restored),
    }
}

/// A disabled service can't run again on its own, so a `last_error` (or
/// `last_run`) left over from a previous enabled period is stale, not
/// current status — surfacing it as a persistent error in Settings ->
/// Background regardless of `enabled` was reported as a bug (#120): once the
/// service is disabled, `daemon.json` is never rewritten, so whatever it
/// last recorded (e.g. a one-off misconfigured `PIPE_DECK_CONFIG_DIR` during
/// manual testing) stuck around forever.
fn stale_state_filter(enabled: bool, state: Option<DaemonStateFile>) -> Option<DaemonStateFile> {
    if enabled {
        state
    } else {
        None
    }
}

fn is_service_enabled() -> bool {
    run_systemctl(&["is-enabled", SERVICE_NAME])
        .map(|output| output.trim() == "enabled")
        .unwrap_or(false)
}

fn is_service_running() -> bool {
    run_systemctl(&["is-active", SERVICE_NAME])
        .map(|output| output.trim() == "active")
        .unwrap_or(false)
}

fn run_systemctl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state(last_error: Option<&str>) -> DaemonStateFile {
        DaemonStateFile {
            pid: 1234,
            last_run: "2026-07-20T14:23:45Z".into(),
            last_error: last_error.map(str::to_string),
            devices_restored: 0,
        }
    }

    #[test]
    fn disabled_service_hides_stale_state() {
        let state = sample_state(Some("failed to read config: missing field `version`"));
        assert!(stale_state_filter(false, Some(state)).is_none());
    }

    #[test]
    fn enabled_service_surfaces_its_recorded_state() {
        let state = sample_state(Some("boom"));
        let result = stale_state_filter(true, Some(state)).expect("state preserved when enabled");
        assert_eq!(result.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn no_recorded_state_stays_none_regardless_of_enabled() {
        assert!(stale_state_filter(true, None).is_none());
        assert!(stale_state_filter(false, None).is_none());
    }

    /// Only the "nothing to remove" branch is safely testable here: the
    /// "unit exists" branch shells out to the real `systemctl --user`
    /// session (see `uninstall_user_service_unit`'s doc comment), which
    /// can't be sandboxed without a fake `systemctl` on `PATH` — same
    /// reason `install_user_service_unit`/`enable_background_service`/
    /// `disable_background_service` have no test coverage either.
    #[test]
    fn uninstall_user_service_unit_is_a_no_op_when_no_unit_file_exists() {
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-uninstall-unit-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create temp XDG_CONFIG_HOME");
        let previous = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", &temp_dir);

        let result = uninstall_user_service_unit();

        match previous {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = fs::remove_dir_all(&temp_dir);

        assert_eq!(result.unwrap(), None);
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d on purpose: hits a *real* PipeWire session, same
    //! convention as `core::engine::effects_ops::live_tests` and
    //! `daemon::ipc::client::live_tests`. Only run via
    //! `cargo test --lib -- --ignored
    //! reconcile_live_effects_state_reloads_a_persisted_chain_after_a_simulated_crash`.
    //! Exercises a disposable `Recovery Test Bus` virtual output this
    //! test creates and removes itself.
    use super::*;
    use crate::backend::linux::pactl;
    use crate::config::store::lock_config_dir_env;
    use crate::core::engine::CoreEngine;
    use crate::core::models::{EffectChainConfig, EffectStage};

    #[test]
    #[ignore]
    fn reconcile_live_effects_state_reloads_a_persisted_chain_after_a_simulated_crash() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let _guard = lock_config_dir_env();
        let temp_dir = std::env::temp_dir().join(format!("pipe-deck-recovery-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &temp_dir);

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");
        let created = engine
            .create_virtual_output("Recovery Test Bus")
            .expect("create disposable test device");

        let cleanup = |engine: &mut CoreEngine| {
            let _ = crate::pipewire::native_host::unload_chain(&created.system_name);
            let _ = engine.remove_virtual_device(&created.system_name);
            std::env::remove_var("PIPE_DECK_CONFIG_DIR");
            let _ = fs::remove_dir_all(&temp_dir);
        };

        // Persist an active chain for this device, exactly as a real Apply
        // would via `set_effect_chain`, but *without* loading it through
        // `native_host` — simulating "the daemon crashed, so nothing is
        // actually loaded right now even though config says it should be".
        let config = EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 4,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        if let Err(error) = ConfigStore::new().set_effect_chain(&created.device_id, &config) {
            cleanup(&mut engine);
            panic!("failed to persist effect chain: {error}");
        }

        assert!(
            !crate::pipewire::native_host::is_loaded(&created.system_name),
            "precondition: nothing should be loaded before reconciliation runs"
        );

        reconcile_live_effects_state();

        let loaded = crate::pipewire::native_host::is_loaded(&created.system_name);
        let sink_live = pactl::sink_exists(&created.system_name).unwrap_or(false);
        cleanup(&mut engine);

        assert!(loaded, "reconcile_live_effects_state did not reload the persisted chain");
        assert!(sink_live, "effects sink did not appear after reconciliation reloaded the chain");
    }
}
