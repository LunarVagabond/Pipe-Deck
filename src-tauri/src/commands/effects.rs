use crate::core::models::{ApplyResult, EffectChainConfig, EffectStage};
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
pub async fn is_effect_chain_live(
    device_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let engine = state.engine.read().await;
    Ok(engine.is_effect_chain_live(&device_id))
}

/// Structural Apply — writes the effects conf, restarts the dedicated
/// filter-chain daemon, and re-links routing. Briefly interrupts audio on
/// the target device only. Per PD-025, the deliberate act of adding/
/// removing/reordering a stage (see `add_effect_stage`/`remove_effect_stage`/
/// `reorder_effect_stages` below) is itself the explicit user action this
/// requires — the frontend no longer gates this behind a separate confirm
/// dialog, just a one-time informational toast.
#[tauri::command]
pub async fn apply_effect_chain_structural<R: tauri::Runtime>(
    device_id: String,
    config: EffectChainConfig,
    app: tauri::AppHandle<R>,
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
pub async fn remove_effect_chain_structural<R: tauri::Runtime>(
    device_id: String,
    app: tauri::AppHandle<R>,
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

/// Node-scoped effects UI entry point (PD-025) — adds `stage` and applies
/// immediately, no separate enable step.
#[tauri::command]
pub async fn add_effect_stage<R: tauri::Runtime>(
    device_id: String,
    stage: EffectStage,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .add_effect_stage(&device_id, stage)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn remove_effect_stage<R: tauri::Runtime>(
    device_id: String,
    stage_id: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .remove_effect_stage(&device_id, &stage_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn reorder_effect_stages<R: tauri::Runtime>(
    device_id: String,
    ordered_stage_ids: Vec<String>,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .reorder_effect_stages(&device_id, &ordered_stage_ids)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{
        add_effect_stage, apply_effect_chain_structural, get_effect_capabilities,
        get_effect_chains, is_effect_chain_live, preflight_effect_chain,
        remove_effect_chain_structural, remove_effect_stage, reorder_effect_stages,
        set_device_effects, set_effect_chain_live_params,
    };
    use crate::commands::test_support::CommandFixture;
    use crate::commands::virtual_device::create_virtual_input;
    use crate::config::ConfigStore;
    use crate::core::models::{ApplyResult, EffectChainConfig, EffectStage};
    use crate::AppState;
    use serde_json::{json, Value};
    use tauri::{async_runtime::block_on, Manager};

    fn eq(id: &str) -> EffectStage {
        EffectStage::Eq5Band {
            id: id.into(),
            eq_sub: 0,
            eq_bass: 0,
            eq_mid: 0,
            eq_treble: 0,
            eq_air: 0,
            output_gain: 0,
        }
    }

    fn chain(ids: &[&str]) -> EffectChainConfig {
        EffectChainConfig {
            stages: ids.iter().map(|id| eq(id)).collect(),
            ..Default::default()
        }
    }

    fn expected_chain(ids: &[&str], live: bool) -> Value {
        json!({
            "stages": ids.iter().map(|id| json!({"kind":"eq5band", "id":id,
                "eq_sub":0,"eq_bass":0,"eq_mid":0,"eq_treble":0,"eq_air":0,"output_gain":0})).collect::<Vec<_>>(),
            "compressor":{"enabled":false,"threshold_db":0,"ratio_x10":0,"attack_ms":0,"release_ms":0}, "limiter":{"enabled":false,"threshold_db":0,"ratio_x10":0,"attack_ms":0,"release_ms":0},
            "noise_gate":{"enabled":false,"threshold_db":0,"ratio_x10":0,"attack_ms":0,"release_ms":0}, "bypassed":false, "live":live
        })
    }

    fn assert_result(result: ApplyResult, message: Option<&str>) {
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success":true,"message":message})
        );
    }

    async fn assert_mic_event(fixture: &CommandFixture) {
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.read().await;
        let mic = engine
            .runtime_graph()
            .devices
            .iter()
            .find(|d| d.id == "virtual-vocal-47")
            .unwrap();
        assert_eq!(mic.system_name, "pipe-deck-vocal-47");
        assert_eq!(mic.label, "Vocal 47");
        fixture.assert_graph_update(engine.runtime_graph());
    }

    async fn mic(fixture: &CommandFixture) {
        let result = create_virtual_input(
            "Vocal 47".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"device_id":"virtual-vocal-47","system_name":"pipe-deck-vocal-47","label":"Vocal 47","multi":false})
        );
        assert_mic_event(fixture).await;
    }

    async fn apply(fixture: &CommandFixture, ids: &[&str]) {
        assert_result(
            apply_effect_chain_structural(
                "virtual-vocal-47".into(),
                chain(ids),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap(),
            Some("Effects applied to Vocal 47"),
        );
        assert_eq!(
            serde_json::to_value(ConfigStore::new().effect_chains().unwrap()).unwrap(),
            json!({"virtual-vocal-47":expected_chain(ids,true)})
        );
        assert_mic_event(fixture).await;
    }

    #[test]
    fn set_device_effects_persists_the_full_chain_without_emitting() {
        let fixture = CommandFixture::new();
        block_on(async {
            mic(&fixture).await;
            let config = EffectChainConfig {
                stages: vec![EffectStage::Eq5Band {
                    id: "voice-eq-29".into(),
                    eq_sub: -2,
                    eq_bass: 3,
                    eq_mid: 5,
                    eq_treble: -7,
                    eq_air: 9,
                    output_gain: -4,
                }],
                bypassed: true,
                live: true,
                ..Default::default()
            };
            assert_result(
                set_device_effects(
                    "virtual-vocal-47".into(),
                    config,
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                None,
            );
            assert_eq!(
                serde_json::to_value(ConfigStore::new().effect_chains().unwrap()).unwrap(),
                json!({"virtual-vocal-47":{
                    "stages":[{"kind":"eq5band","id":"voice-eq-29","eq_sub":-2,"eq_bass":3,"eq_mid":5,"eq_treble":-7,"eq_air":9,"output_gain":-4}],
                    "compressor":{"enabled":false,"threshold_db":0,"ratio_x10":0,"attack_ms":0,"release_ms":0},"limiter":{"enabled":false,"threshold_db":0,"ratio_x10":0,"attack_ms":0,"release_ms":0},"noise_gate":{"enabled":false,"threshold_db":0,"ratio_x10":0,"attack_ms":0,"release_ms":0},"bypassed":true,"live":false
                }})
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn set_device_effects_maps_missing_device_to_string() {
        let fixture = CommandFixture::new();
        let error = block_on(set_device_effects(
            "absent-mic-19".into(),
            chain(&["eq-3"]),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "pipewire adapter error: device not found: absent-mic-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_effect_chains_returns_empty_and_populated_payloads_without_emitting() {
        let fixture = CommandFixture::new();
        block_on(async {
            assert_eq!(
                serde_json::to_value(
                    get_effect_chains(fixture.app.state::<AppState>())
                        .await
                        .unwrap()
                )
                .unwrap(),
                json!({})
            );
            assert!(fixture.take_graph_updates().is_empty());
            mic(&fixture).await;
            assert_result(
                set_device_effects(
                    "virtual-vocal-47".into(),
                    chain(&["eq-61"]),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                None,
            );
            assert!(fixture.take_graph_updates().is_empty());
            assert_eq!(
                serde_json::to_value(
                    get_effect_chains(fixture.app.state::<AppState>())
                        .await
                        .unwrap()
                )
                .unwrap(),
                json!({"virtual-vocal-47":expected_chain(&["eq-61"],false)})
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn get_effect_chains_maps_config_read_error_to_string() {
        let fixture = CommandFixture::new();
        let path = ConfigStore::new().config_dir().join("config.yaml");
        std::fs::create_dir(&path).unwrap();
        let error = block_on(get_effect_chains(fixture.app.state::<AppState>())).unwrap_err();
        assert_eq!(
            error,
            format!("config error: failed to read config: {path:?}: Is a directory (os error 21)")
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_effect_capabilities_returns_the_system_probe_without_emitting() {
        let fixture = CommandFixture::new();
        // The mock backend inherits the real filesystem capability probe.
        let expected =
            serde_json::to_value(crate::pipewire::fx_capability::probe_capabilities()).unwrap();
        let result = block_on(get_effect_capabilities(fixture.app.state::<AppState>())).unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), expected);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn preflight_effect_chain_reports_valid_and_blocked_payloads_without_emitting() {
        let fixture = CommandFixture::new();
        block_on(async {
            let valid =
                preflight_effect_chain(chain(&["neutral-eq-31"]), fixture.app.state::<AppState>())
                    .await
                    .unwrap();
            assert_eq!(
                serde_json::to_value(valid).unwrap(),
                json!({"ok":true,"warnings":[],"blocking_reasons":[]})
            );
            assert!(fixture.take_graph_updates().is_empty());
            let mut config = chain(&["neutral-eq-31"]);
            config.limiter.enabled = true;
            let blocked = preflight_effect_chain(config, fixture.app.state::<AppState>())
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_value(blocked).unwrap(),
                json!({"ok":false,"warnings":[],"blocking_reasons":["Limiter has no supported backing plugin on this system yet — disabled until one is available"]})
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn is_effect_chain_live_reports_missing_inactive_live_and_removed_without_emitting() {
        let fixture = CommandFixture::new();
        block_on(async {
            assert_eq!(
                is_effect_chain_live("absent-mic-19".into(), fixture.app.state::<AppState>()).await,
                Ok(false)
            );
            assert!(fixture.take_graph_updates().is_empty());
            mic(&fixture).await;
            assert_eq!(
                is_effect_chain_live("virtual-vocal-47".into(), fixture.app.state::<AppState>())
                    .await,
                Ok(false)
            );
            assert!(fixture.take_graph_updates().is_empty());
            apply(&fixture, &["eq-71"]).await;
            assert_eq!(
                is_effect_chain_live("virtual-vocal-47".into(), fixture.app.state::<AppState>())
                    .await,
                Ok(true)
            );
            assert!(fixture.take_graph_updates().is_empty());
            assert_result(
                remove_effect_chain_structural(
                    "virtual-vocal-47".into(),
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                Some("Effects removed from Vocal 47"),
            );
            assert_mic_event(&fixture).await;
            assert_eq!(
                is_effect_chain_live("virtual-vocal-47".into(), fixture.app.state::<AppState>())
                    .await,
                Ok(false)
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn apply_effect_chain_structural_persists_live_state_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            mic(&fixture).await;
            apply(&fixture, &["eq-83"]).await;
        });
    }

    #[test]
    fn apply_effect_chain_structural_maps_missing_device_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(apply_effect_chain_structural(
            "absent-mic-19".into(),
            chain(&["eq-3"]),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: device not found: absent-mic-19");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn remove_effect_chain_structural_removes_live_state_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            mic(&fixture).await;
            apply(&fixture, &["eq-43"]).await;
            assert_result(
                remove_effect_chain_structural(
                    "virtual-vocal-47".into(),
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                Some("Effects removed from Vocal 47"),
            );
            assert!(ConfigStore::new().effect_chains().unwrap().is_empty());
            assert_mic_event(&fixture).await;
        });
    }

    #[test]
    fn remove_effect_chain_structural_maps_missing_device_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(remove_effect_chain_structural(
            "absent-mic-19".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: device not found: absent-mic-19");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_effect_chain_live_params_returns_mock_payload_without_emitting() {
        let fixture = CommandFixture::new();
        assert_result(
            block_on(set_effect_chain_live_params(
                "source-mic-filtered".into(),
                chain(&["eq-59"]),
                fixture.app.state::<AppState>(),
            ))
            .unwrap(),
            Some("live params updated (mock)"),
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_effect_chain_live_params_maps_missing_device_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        let error = block_on(set_effect_chain_live_params(
            "absent-mic-19".into(),
            chain(&["eq-59"]),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: device not found: absent-mic-19");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn add_effect_stage_appends_the_requested_stage_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            mic(&fixture).await;
            apply(&fixture, &["eq-11"]).await;
            assert_result(
                add_effect_stage(
                    "virtual-vocal-47".into(),
                    eq("eq-23"),
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                Some("Effects applied to Vocal 47"),
            );
            assert_eq!(
                serde_json::to_value(ConfigStore::new().effect_chains().unwrap()).unwrap(),
                json!({"virtual-vocal-47":expected_chain(&["eq-11","eq-23"],true)})
            );
            assert_mic_event(&fixture).await;
        });
    }

    #[test]
    fn add_effect_stage_maps_missing_device_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(add_effect_stage(
            "absent-mic-19".into(),
            eq("eq-23"),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: device not found: absent-mic-19");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn remove_effect_stage_keeps_the_other_stage_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            mic(&fixture).await;
            apply(&fixture, &["eq-11", "eq-23"]).await;
            assert_result(
                remove_effect_stage(
                    "virtual-vocal-47".into(),
                    "eq-11".into(),
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                Some("Effects applied to Vocal 47"),
            );
            assert_eq!(
                serde_json::to_value(ConfigStore::new().effect_chains().unwrap()).unwrap(),
                json!({"virtual-vocal-47":expected_chain(&["eq-23"],true)})
            );
            assert_mic_event(&fixture).await;
        });
    }

    #[test]
    fn remove_effect_stage_maps_missing_device_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(remove_effect_stage(
            "absent-mic-19".into(),
            "eq-11".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: device not found: absent-mic-19");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn reorder_effect_stages_reorders_and_preserves_omitted_stages_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            mic(&fixture).await;
            apply(&fixture, &["eq-11", "eq-23", "eq-37"]).await;
            assert_result(
                reorder_effect_stages(
                    "virtual-vocal-47".into(),
                    vec!["eq-23".into(), "eq-11".into()],
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
                Some("Effects applied to Vocal 47"),
            );
            assert_eq!(
                serde_json::to_value(ConfigStore::new().effect_chains().unwrap()).unwrap(),
                json!({"virtual-vocal-47":expected_chain(&["eq-23","eq-11","eq-37"],true)})
            );
            assert_mic_event(&fixture).await;
        });
    }

    #[test]
    fn reorder_effect_stages_maps_missing_device_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(reorder_effect_stages(
            "absent-mic-19".into(),
            vec!["eq-23".into()],
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: device not found: absent-mic-19");
        assert!(fixture.take_graph_updates().is_empty());
    }
}
