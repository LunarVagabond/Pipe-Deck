use crate::core::models::ApplyResult;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn set_stream_targets(
    stream_id: String,
    target_device_ids: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_stream_targets(&stream_id, &target_device_ids)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn set_stream_target(
    stream_id: String,
    target_device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_stream_target(&stream_id, &target_device_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn set_device_route(
    source_device_id: String,
    target_device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_device_route(&source_device_id, &target_device_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn set_device_targets(
    source_device_id: String,
    target_device_ids: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_device_targets(&source_device_id, &target_device_ids)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn clear_stream_target(
    stream_id: String,
    previous_target_device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .clear_stream_target(&stream_id, Some(&previous_target_device_id))
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn undo_last_routing(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .undo_last_routing()
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn can_undo_routing(state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.engine.read().await;
    Ok(engine.can_undo_routing())
}

#[tauri::command]
pub async fn get_last_error(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let engine = state.engine.read().await;
    Ok(engine.last_error().map(str::to_string))
}
