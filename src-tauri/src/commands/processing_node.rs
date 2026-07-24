use crate::core::models::{ApplyResult, ProcessingNode, ProcessingNodeSpecKind};
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
