use crate::config::{ConfigStore, ThemeStore};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub fn get_config() -> crate::core::models::AppConfig {
    ConfigStore::new().load_config().unwrap_or_else(|_| ConfigStore::default_config())
}

#[tauri::command]
pub fn list_profiles() -> Vec<crate::core::models::ProfileIndexEntry> {
    ConfigStore::new().list_profiles().unwrap_or_default()
}

#[tauri::command]
pub async fn set_device_alias(
    system_name: String,
    alias: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .apply_device_alias(&system_name, &alias)
        .map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub fn set_show_system_streams(show: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_show_system_streams(show)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_auto_apply_rules(enabled: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_auto_apply_rules(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_sidebar_collapsed(collapsed: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_sidebar_collapsed(collapsed)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_themes() -> Vec<crate::core::models::ResolvedScheme> {
    let config_store = ConfigStore::new();
    ThemeStore::new(config_store.config_dir().clone()).list_schemes()
}

#[tauri::command]
pub fn set_theme_mode(mode: String) -> Result<(), String> {
    ConfigStore::new()
        .set_theme_mode(&mode)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_dark_scheme(id: String) -> Result<(), String> {
    ConfigStore::new()
        .set_dark_scheme(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_light_scheme(id: String) -> Result<(), String> {
    ConfigStore::new()
        .set_light_scheme(&id)
        .map_err(|error| error.to_string())
}
