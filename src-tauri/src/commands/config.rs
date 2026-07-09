use crate::config::ConfigStore;

#[tauri::command]
pub fn get_config() -> crate::core::models::AppConfig {
    ConfigStore::new().load_config().unwrap_or_else(|_| {
        crate::core::models::AppConfig {
            version: 1,
            active_profile: None,
            profile_index: vec![],
            preferences: crate::core::models::Preferences::default(),
            devices: std::collections::HashMap::new(),
        }
    })
}

#[tauri::command]
pub fn list_profiles() -> Vec<crate::core::models::ProfileIndexEntry> {
    ConfigStore::new().list_profiles().unwrap_or_default()
}

#[tauri::command]
pub fn set_device_alias(system_name: String, alias: String) -> Result<(), String> {
    ConfigStore::new()
        .set_device_alias(&system_name, &alias)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_show_system_streams(show: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_show_system_streams(show)
        .map_err(|error| error.to_string())
}
