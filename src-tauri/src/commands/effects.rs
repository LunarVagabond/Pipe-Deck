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

#[tauri::command]
pub async fn is_effect_chain_live(device_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.engine.read().await;
    Ok(engine.is_effect_chain_live(&device_id))
}

/// Structural Apply — writes the effects conf, restarts the dedicated
/// filter-chain daemon, and re-links routing. Briefly interrupts audio on
/// the target device only; the frontend must confirm with the user before
/// calling this (see the Effects view's Apply flow).
#[tauri::command]
pub async fn apply_effect_chain_structural(
    device_id: String,
    config: EffectChainConfig,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .apply_effect_chain_structural(&device_id, &config)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn remove_effect_chain_structural(
    device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .remove_effect_chain_structural(&device_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

/// Live Params — pushes an EQ/gain value straight to the already-running
/// effects node, no restart, no confirm needed. Safe to call on every slider
/// tick once live effects are enabled for this device.
#[tauri::command]
pub async fn set_effect_chain_live_params(
    device_id: String,
    config: EffectChainConfig,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    engine
        .set_effect_chain_live_params(&device_id, &config)
        .map_err(|error| error.to_string())
}
