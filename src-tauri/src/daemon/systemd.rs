//! Native systemd user-session control via `zbus` (#425) — replaces
//! `daemon/mod.rs`'s `run_systemctl` (`Command::new("systemctl").arg("--user")...`)
//! with direct calls against `org.freedesktop.systemd1.Manager`/`.Unit` over
//! the session D-Bus. Unlike the `backend::linux` PipeWire modules'
//! native-first/CLI-fallback shape, there's no fallback here: `systemctl`
//! itself is just a D-Bus client for exactly this API, so a session bus
//! being unreachable would break the `systemctl` shellout identically — a
//! CLI fallback would buy nothing.
//!
//! Every function opens its own short-lived `Connection::session()` rather
//! than caching one process-wide — unlike the PipeWire native modules (which
//! push to a persistent connection because they're on hot per-frame-adjacent
//! paths), unit enable/disable/status checks happen at most a few times per
//! app session (Settings toggle, startup status read), so the extra
//! round-trip cost of a fresh connection per call isn't worth the lifecycle
//! complexity a cached one would add.

use zbus::blocking::Connection;
use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;

/// `(unit_file, change_type, target)` — each entry `systemd.EnableUnitFiles`/
/// `DisableUnitFiles` returns describing a symlink it created or removed.
/// Unused here (this module only cares whether the call succeeded), just
/// named so the proxy method signatures don't spell out a bare 3-tuple.
type UnitFileChange = (String, String, String);

#[proxy(
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1",
    interface = "org.freedesktop.systemd1.Manager"
)]
trait Manager {
    fn reload(&self) -> zbus::Result<()>;

    #[zbus(name = "EnableUnitFiles")]
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<UnitFileChange>)>;

    #[zbus(name = "DisableUnitFiles")]
    fn disable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
    ) -> zbus::Result<Vec<UnitFileChange>>;

    #[zbus(name = "StartUnit")]
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(name = "StopUnit")]
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(name = "GetUnitFileState")]
    fn get_unit_file_state(&self, file: &str) -> zbus::Result<String>;

    #[zbus(name = "GetUnit")]
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    default_service = "org.freedesktop.systemd1",
    interface = "org.freedesktop.systemd1.Unit"
)]
trait Unit {
    #[zbus(property, name = "ActiveState")]
    fn active_state(&self) -> zbus::Result<String>;
}

fn manager(connection: &Connection) -> Result<ManagerProxyBlocking<'_>, String> {
    ManagerProxyBlocking::new(connection).map_err(|error| error.to_string())
}

pub fn reload() -> Result<(), String> {
    let connection = Connection::session().map_err(|error| error.to_string())?;
    let manager = manager(&connection)?;
    manager.reload().map_err(|error| error.to_string())
}

/// `systemctl --user enable --now <unit>` — enables the unit's install
/// symlinks, then starts it. `mode = "replace"` matches `systemctl`'s own
/// default job mode (queue the job, replacing any conflicting queued job
/// for the same unit, rather than failing outright).
pub fn enable_and_start(unit: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| error.to_string())?;
    let manager = manager(&connection)?;
    manager
        .enable_unit_files(&[unit], false, true)
        .map_err(|error| error.to_string())?;
    manager
        .start_unit(unit, "replace")
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// `systemctl --user disable --now <unit>` — stops the unit, then removes
/// its install symlinks. Errors from either step are surfaced, but (like the
/// `pactl`/`pw_link` native modules) each half still runs independently —
/// this doesn't try to roll back a successful stop if disable then fails.
pub fn disable_and_stop(unit: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| error.to_string())?;
    let manager = manager(&connection)?;
    let stop_result = manager
        .stop_unit(unit, "replace")
        .map(|_| ())
        .map_err(|error| error.to_string());
    let disable_result = manager
        .disable_unit_files(&[unit], false)
        .map(|_| ())
        .map_err(|error| error.to_string());
    stop_result.and(disable_result)
}

/// `systemctl --user is-enabled <unit>` — `false` for "disabled", "static",
/// "not-found", or any D-Bus error (unit never installed, no session bus,
/// ...), matching the old shellout's `unwrap_or(false)` tolerance exactly.
pub fn is_enabled(unit: &str) -> bool {
    let Ok(connection) = Connection::session() else {
        return false;
    };
    let Ok(manager) = manager(&connection) else {
        return false;
    };
    manager
        .get_unit_file_state(unit)
        .map(|state| state == "enabled")
        .unwrap_or(false)
}

/// `systemctl --user is-active <unit>` — `false` for "inactive", "failed",
/// "unit never loaded" (`GetUnit` itself errors with `NoSuchUnit` in that
/// case), or any other D-Bus error.
pub fn is_active(unit: &str) -> bool {
    is_active_inner(unit).unwrap_or(false)
}

fn is_active_inner(unit: &str) -> zbus::Result<bool> {
    let connection = Connection::session()?;
    let unit_path = ManagerProxyBlocking::new(&connection)?.get_unit(unit)?;
    let unit_proxy = UnitProxyBlocking::builder(&connection)
        .path(unit_path)?
        .build()?;
    Ok(unit_proxy.active_state()? == "active")
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits the real user session D-Bus/systemd instance, same
    //! rationale as every other `live_tests` module in this codebase.
    //!
    //! Deliberately writes its disposable unit file straight into the *real*
    //! `~/.config/systemd/user/` — `daemon::mod`'s own tests can safely
    //! redirect `XDG_CONFIG_HOME` to a temp dir because they only care about
    //! file-write behavior, but that redirect is invisible to the already-running
    //! systemd --user *process* this test actually talks to over D-Bus: an env
    //! var this test process sets has no effect on where that separate process
    //! looks for unit files. Safe here specifically because the unit name
    //! (`pipe-deck-zbus-live-test.service`) is never `pipe-deck-daemon.service`
    //! and this test removes it (+ reloads) in every path, including on assertion
    //! failure (via a guard's `Drop`).
    use super::*;
    use std::fs;

    const TEST_UNIT: &str = "pipe-deck-zbus-live-test.service";

    fn real_user_systemd_dir() -> std::path::PathBuf {
        std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/systemd/user"))
            .expect("HOME must be set to run this live test")
    }

    struct DisposableUnit {
        path: std::path::PathBuf,
    }

    impl DisposableUnit {
        fn install() -> Self {
            let dir = real_user_systemd_dir();
            fs::create_dir_all(&dir).expect("create real user systemd dir");
            let path = dir.join(TEST_UNIT);
            fs::write(
                &path,
                "[Unit]\nDescription=Pipe Deck zbus live test (safe to remove)\n\n\
                 [Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/bin/true\n\n\
                 [Install]\nWantedBy=default.target\n",
            )
            .expect("write disposable test unit");
            reload().expect("reload after writing disposable unit");
            Self { path }
        }
    }

    impl Drop for DisposableUnit {
        fn drop(&mut self) {
            let _ = disable_and_stop(TEST_UNIT);
            let _ = fs::remove_file(&self.path);
            let _ = reload();
        }
    }

    #[test]
    #[ignore]
    fn enables_starts_and_reports_state_matching_systemctls_own_readback() {
        let _unit = DisposableUnit::install();

        assert!(!is_enabled(TEST_UNIT), "should start disabled");
        assert!(!is_active(TEST_UNIT), "should start inactive");

        enable_and_start(TEST_UNIT).expect("enable_and_start should succeed");

        assert!(
            is_enabled(TEST_UNIT),
            "expected the unit to report enabled after enable_and_start"
        );
        assert!(
            is_active(TEST_UNIT),
            "expected the unit to report active after enable_and_start"
        );
        assert_eq!(
            systemctl_is_active(TEST_UNIT),
            "active",
            "systemctl's own independent readback should agree the unit is active"
        );

        disable_and_stop(TEST_UNIT).expect("disable_and_stop should succeed");

        assert!(
            !is_enabled(TEST_UNIT),
            "expected the unit to report disabled after disable_and_stop"
        );
        assert!(
            !is_active(TEST_UNIT),
            "expected the unit to report inactive after disable_and_stop"
        );
        assert_ne!(
            systemctl_is_active(TEST_UNIT),
            "active",
            "systemctl's own independent readback should agree the unit is no longer active"
        );
    }

    /// Independent verification via the actual `systemctl` binary — not this
    /// module's own `is_active`, which is exactly what this test is trying to
    /// validate rather than assume.
    fn systemctl_is_active(unit: &str) -> String {
        let output = std::process::Command::new("systemctl")
            .args(["--user", "is-active", unit])
            .output()
            .expect("failed to run systemctl for independent verification");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}
