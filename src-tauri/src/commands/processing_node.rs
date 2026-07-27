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
pub async fn update_processing_node_volume(
    node_id: String,
    volume_percent: u8,
    muted: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_volume(&node_id, volume_percent, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_processing_node_eq_params(
    node_id: String,
    eq_sub: i32,
    eq_bass: i32,
    eq_mid: i32,
    eq_treble: i32,
    eq_air: i32,
    output_gain: i32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_eq_params(&node_id, eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_delay_params(
    node_id: String,
    delay_ms: i32,
    feedback_percent: i32,
    feedforward_percent: i32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_delay_params(&node_id, delay_ms, feedback_percent, feedforward_percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_limiter_params(
    node_id: String,
    ceiling_db: i32,
    floor_db: i32,
    symmetric: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_limiter_params(&node_id, ceiling_db, floor_db, symmetric)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_hpf_params(
    node_id: String,
    freq_hz: i32,
    resonance_x10: i32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_hpf_params(&node_id, freq_hz, resonance_x10)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_reverb_params(
    node_id: String,
    mix_percent: i32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_reverb_params(&node_id, mix_percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_widener_params(
    node_id: String,
    width_percent: i32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_widener_params(&node_id, width_percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn set_processing_node_bypassed(
    node_id: String,
    bypassed: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_processing_node_bypassed(&node_id, bypassed)
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
