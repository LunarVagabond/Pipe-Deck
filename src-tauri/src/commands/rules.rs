use crate::core::models::{Rule, SimulationResult};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_rules(state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let engine = state.engine.read().await;
    engine.list_rules().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_rule(
    rule: Rule,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.save_rule(rule).map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_rule(
    rule_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .delete_rule(&rule_id)
        .map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn toggle_rule(
    rule_id: String,
    enabled: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .toggle_rule(&rule_id, enabled)
        .map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn simulate_rules(state: State<'_, AppState>) -> Result<Vec<SimulationResult>, String> {
    let engine = state.engine.read().await;
    Ok(engine.simulate_rules())
}
