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

#[cfg(test)]
mod tests {
    use super::set_restore_on_startup;
    use crate::commands::test_support::CommandFixture;
    use crate::config::ConfigStore;

    #[test]
    fn restore_on_startup_persists_both_states_without_emitting() {
        let fixture = CommandFixture::new();
        for enabled in [false, true] {
            assert_eq!(set_restore_on_startup(enabled), Ok(()));
            assert_eq!(
                ConfigStore::new()
                    .load_config()
                    .unwrap()
                    .preferences
                    .restore_on_startup,
                enabled
            );
            assert!(fixture.take_graph_updates().is_empty());
        }
    }

    #[test]
    fn restore_on_startup_maps_write_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            set_restore_on_startup(false),
            Err("failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
