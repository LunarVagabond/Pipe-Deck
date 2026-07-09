use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<crate::core::models::PluginStatus>, String> {
    let engine = state.engine.read().await;
    Ok(engine.list_plugins())
}

#[tauri::command]
pub async fn set_plugin_enabled(
    plugin_id: String,
    enabled: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.set_plugin_enabled(&plugin_id, enabled)?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn grant_plugin_capabilities(
    plugin_id: String,
    capabilities: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.grant_plugin_capabilities(&plugin_id, capabilities)?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn list_plugin_ui_panels(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::models::PluginUiPanel>, String> {
    let engine = state.engine.read().await;
    Ok(engine
        .plugin_ui_panels()
        .into_iter()
        .map(|(_, panel)| panel)
        .collect())
}
