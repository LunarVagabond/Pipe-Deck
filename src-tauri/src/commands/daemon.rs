use crate::config::ConfigStore;
use crate::core::models::DaemonStatus;
use crate::daemon;

#[tauri::command]
pub fn get_daemon_status() -> DaemonStatus {
    daemon::get_status()
}

#[tauri::command]
pub fn enable_background_restore() -> Result<(), String> {
    daemon::enable_background_service()
}

#[tauri::command]
pub fn disable_background_restore() -> Result<(), String> {
    daemon::disable_background_service()
}

#[tauri::command]
pub fn set_restore_on_startup(enabled: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_restore_on_startup(enabled)
        .map_err(|error| error.to_string())
}
