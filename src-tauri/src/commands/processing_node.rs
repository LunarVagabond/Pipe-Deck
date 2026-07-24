use crate::core::models::{ApplyResult, PortDirection, ProcessingNode, ProcessingNodeSpecKind};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_processing_node(
    label: String,
    kind: ProcessingNodeSpecKind,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ProcessingNode, String> {
    let mut engine = state.engine.write().await;
    let node = engine
        .create_processing_node(&label, kind)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(node)
}

#[tauri::command]
pub async fn remove_processing_node(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .remove_processing_node(&id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn connect_processing_node_port(
    node_id: String,
    direction: PortDirection,
    peer_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .connect_processing_node_port(&node_id, direction, &peer_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_input_gain(
    node_id: String,
    port_index: u32,
    gain_percent: u8,
    muted: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_input_gain(&node_id, port_index, gain_percent, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn disconnect_processing_node_port(
    node_id: String,
    direction: PortDirection,
    port_index: u32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .disconnect_processing_node_port(&node_id, direction, port_index)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}
