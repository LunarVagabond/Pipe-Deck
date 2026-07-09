use crate::core::models::RuntimeGraph;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_runtime_graph(state: State<'_, AppState>) -> Result<RuntimeGraph, String> {
    let mut engine = state.engine.write().await;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    Ok(engine.runtime_graph().clone())
}
