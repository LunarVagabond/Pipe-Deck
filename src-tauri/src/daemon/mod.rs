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

const SERVICE_NAME: &str = "pipe-deck-daemon.service";

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
                        write_status(pid, &started, None, devices_restored);
                        return 0;
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
    0
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

pub fn install_user_service_unit() -> Result<(), String> {
    let daemon_path = daemon_binary_path()
        .ok_or_else(|| "pipe-deck-daemon binary not found".to_string())?;
    let unit_dir = user_systemd_dir();
    fs::create_dir_all(&unit_dir).map_err(|error| error.to_string())?;

    let unit = bundled_service_unit().replace(
        "ExecStart=/usr/bin/pipe-deck-daemon",
        &format!("ExecStart={}", daemon_path.display()),
    );
    fs::write(unit_dir.join(SERVICE_NAME), unit).map_err(|error| error.to_string())?;
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

pub fn get_status() -> DaemonStatus {
    let state = read_status();
    let enabled = is_service_enabled();
    let running = is_service_running();

    DaemonStatus {
        running,
        enabled,
        pid: state.as_ref().map(|value| value.pid),
        last_run: state.as_ref().map(|value| value.last_run.clone()),
        last_error: state.as_ref().and_then(|value| value.last_error.clone()),
        devices_restored: state.as_ref().map(|value| value.devices_restored),
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
