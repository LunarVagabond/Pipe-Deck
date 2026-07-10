use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn set_device_volume(
    device_id: String,
    percent: u8,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_device_volume(&device_id, percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_device_mute(
    device_id: String,
    muted: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_device_mute(&device_id, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_stream_volume(
    stream_id: String,
    percent: u8,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_stream_volume(&stream_id, percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_stream_mute(
    stream_id: String,
    muted: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_stream_mute(&stream_id, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}
