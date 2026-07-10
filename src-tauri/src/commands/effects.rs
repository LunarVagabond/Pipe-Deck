use crate::core::models::{ApplyResult, EffectChainConfig};
use crate::AppState;
use std::collections::HashMap;
use tauri::State;

#[tauri::command]
pub async fn set_device_effects(
    device_id: String,
    config: EffectChainConfig,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_device_effects(&device_id, config)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
pub async fn get_effect_chains(
    state: State<'_, AppState>,
) -> Result<HashMap<String, EffectChainConfig>, String> {
    let engine = state.engine.read().await;
    engine
        .get_effect_chains()
        .map_err(|error| error.to_string())
}
