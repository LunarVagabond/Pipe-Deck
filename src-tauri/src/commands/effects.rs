use crate::core::models::{ApplyResult, EffectChainConfig};
use crate::pipewire::fx_capability::FxCapabilities;
use crate::pipewire::fx_validate::PreflightResult;
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

#[tauri::command]
pub async fn get_effect_capabilities(state: State<'_, AppState>) -> Result<FxCapabilities, String> {
    let engine = state.engine.read().await;
    Ok(engine.get_effect_capabilities())
}

#[tauri::command]
pub async fn preflight_effect_chain(
    config: EffectChainConfig,
    state: State<'_, AppState>,
) -> Result<PreflightResult, String> {
    let engine = state.engine.read().await;
    Ok(engine.preflight_effect_chain(&config))
}
