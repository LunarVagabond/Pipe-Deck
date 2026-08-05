use crate::config::{ConfigStore, ProfileStore, ThemeStore};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub fn get_config() -> crate::core::models::AppConfig {
    ConfigStore::new().load_config().unwrap_or_else(|_| ConfigStore::default_config())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPaths {
    config_dir: String,
    profiles_dir: String,
    plugins_dir: String,
}

#[tauri::command]
pub fn get_config_paths() -> ConfigPaths {
    let config_store = ConfigStore::new();
    let profiles_dir = ProfileStore::new(config_store.config_dir().clone()).profiles_dir();
    let plugins_dir = crate::plugins::registry::user_plugins_dir();
    ConfigPaths {
        config_dir: config_store.config_dir().display().to_string(),
        profiles_dir: profiles_dir.display().to_string(),
        plugins_dir: plugins_dir.display().to_string(),
    }
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
pub fn set_onboarding_dismissed(dismissed: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_onboarding_dismissed(dismissed)
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

/// Persists the user's close-button choice. `apply_now` distinguishes the
/// two callers: the one-time prompt (#295) answers *for* a click that's
/// already in flight, so it passes `true` to also perform the hide/exit
/// immediately — otherwise the window is left in limbo (not hidden, not
/// quit). A plain preference change from Settings passes `false`, since no
/// close is in progress there.
#[tauri::command]
pub fn set_close_behavior(behavior: String, apply_now: bool, app: tauri::AppHandle) -> Result<(), String> {
    ConfigStore::new()
        .set_close_behavior(&behavior)
        .map_err(|error| error.to_string())?;
    if apply_now {
        crate::tray::apply_close_behavior(&app, &behavior);
    }
    Ok(())
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

#[tauri::command]
pub fn set_notice_duration_ms(ms: u32) -> Result<(), String> {
    ConfigStore::new()
        .set_notice_duration_ms(ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_update_channel(channel: String) -> Result<(), String> {
    ConfigStore::new()
        .set_update_channel(&channel)
        .map_err(|error| error.to_string())
}
