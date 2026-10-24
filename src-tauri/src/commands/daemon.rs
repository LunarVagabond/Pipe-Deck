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
    use super::*;
    use crate::commands::test_support::CommandFixture;
    use serde_json::json;

    #[test]
    fn daemon_status_returns_the_complete_inactive_payload_without_emitting() {
        let mut fixture = CommandFixture::new();
        fixture.isolate_daemon();
        assert_eq!(
            serde_json::to_value(get_daemon_status()).unwrap(),
            json!({
                "running": false, "enabled": false, "pid": null,
                "last_run": null, "last_error": null, "devices_restored": null
            })
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn disable_background_restore_persists_false_and_maps_write_error_without_emitting() {
        let mut fixture = CommandFixture::new();
        fixture.isolate_daemon();
        ConfigStore::new().set_background_restore(true).unwrap();
        assert_eq!(disable_background_restore(), Ok(()));
        assert!(
            !ConfigStore::new()
                .load_config()
                .unwrap()
                .preferences
                .background_restore
        );
        assert!(fixture.take_graph_updates().is_empty());

        fixture.fail_config_writes();
        assert_eq!(
            disable_background_restore(),
            Err("failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn enable_background_restore_maps_setup_error_without_emitting() {
        let mut fixture = CommandFixture::new();
        fixture.isolate_daemon();
        ConfigStore::new().set_background_restore(false).unwrap();
        let unit_dir = daemon::user_systemd_dir();
        std::fs::create_dir_all(unit_dir.parent().unwrap()).unwrap();
        std::fs::write(&unit_dir, b"service directory blocker").unwrap();
        assert_eq!(
            enable_background_restore(),
            Err("File exists (os error 17)".into())
        );
        assert!(
            !ConfigStore::new()
                .load_config()
                .unwrap()
                .preferences
                .background_restore
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

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
