use crate::core::models::{ApplyResult, VirtualDeviceResult};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_virtual_output(
    name: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<VirtualDeviceResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .create_virtual_output(&name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn create_virtual_input(
    name: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<VirtualDeviceResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .create_virtual_input(&name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn create_virtual_multi_output(
    name: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<VirtualDeviceResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .create_virtual_multi_output(&name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn remove_virtual_device(
    system_name: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .remove_virtual_device(&system_name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn enable_stream_mic_passthrough(
    stream_id: String,
    mic_device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .enable_stream_mic_passthrough(&stream_id, &mic_device_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}
