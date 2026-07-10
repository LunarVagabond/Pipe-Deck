use crate::config::ConfigStore;
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
    ConfigStore::new()
        .set_device_alias(&system_name, &alias)
        .map_err(|error| error.to_string())?;

    if system_name.starts_with("pipe-deck-") && !system_name.starts_with("pipe-deck-feed-") {
        let _ = crate::pipewire::pactl::sync_feed_sink_for_virtual_input(&system_name, &alias);
    }

    {
        let engine = state.engine.read().await;
        if system_name.starts_with("pipe-deck-") && !system_name.starts_with("pipe-deck-feed-") {
            let _ = engine.virtual_registry().set_label(&system_name, &alias);
        }
    }

    let mut engine = state.engine.write().await;
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
