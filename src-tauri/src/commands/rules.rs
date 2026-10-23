use crate::core::models::{ApplyResult, Rule, SimulationResult};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_rules(state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let engine = state.engine.read().await;
    engine.list_rules().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_rule<R: tauri::Runtime>(
    rule: Rule,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.save_rule(rule).map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_rule<R: tauri::Runtime>(
    rule_id: String,
    app: tauri::AppHandle<R>,
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
pub async fn toggle_rule<R: tauri::Runtime>(
    rule_id: String,
    enabled: bool,
    app: tauri::AppHandle<R>,
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
pub async fn simulate_rules(
    priority_overrides: Option<std::collections::HashMap<String, i32>>,
    state: State<'_, AppState>,
) -> Result<Vec<SimulationResult>, String> {
    let engine = state.engine.read().await;
    Ok(engine.simulate_rules(&priority_overrides.unwrap_or_default()))
}

#[tauri::command]
pub async fn apply_rules<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    engine
        .apply_desired_routing()
        .map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(ApplyResult {
        success: true,
        message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::CommandFixture;
    use crate::config::ConfigStore;
    use crate::core::models::{RuleAction, RuleCondition, RuleSafeguards};
    use serde_json::{json, Value};
    use tauri::{async_runtime::block_on, Manager};

    fn rule() -> Rule {
        Rule {
            id: "desk-rule".into(),
            name: "Desk audio".into(),
            enabled: true,
            priority: 31,
            conditions: vec![RuleCondition::AppName {
                value: "Discord".into(),
            }],
            action: RuleAction {
                target_system_name: Some("sink-speakers".into()),
                target_system_names: vec![],
            },
            safeguards: RuleSafeguards::default(),
        }
    }

    fn other_rule() -> Rule {
        Rule {
            id: "other-rule".into(),
            name: "Headphone audio".into(),
            priority: 7,
            action: RuleAction {
                target_system_name: Some("sink-headphones".into()),
                target_system_names: vec![],
            },
            ..rule()
        }
    }

    fn assert_event(fixture: &CommandFixture, target: &str) {
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.blocking_read();
        let graph = engine.runtime_graph();
        let stream = graph
            .streams
            .iter()
            .find(|stream| stream.id == "stream-discord")
            .unwrap();
        assert_eq!(stream.app_name, "Discord");
        assert_eq!(stream.current_target.as_deref(), Some(target));
        fixture.assert_graph_update(graph);
    }

    #[test]
    fn list_rules_returns_empty_and_complete_priority_ordered_rules_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(list_rules(fixture.app.state::<AppState>())).unwrap(),
            vec![]
        );
        assert!(fixture.take_graph_updates().is_empty());
        ConfigStore::new().save_rule(other_rule()).unwrap();
        ConfigStore::new().save_rule(rule()).unwrap();
        assert_eq!(
            block_on(list_rules(fixture.app.state::<AppState>())).unwrap(),
            vec![rule(), other_rule()]
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_rules_maps_read_error_without_emitting() {
        let fixture = CommandFixture::new();
        let path = ConfigStore::new().config_dir().join("config.yaml");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            block_on(list_rules(fixture.app.state::<AppState>())).unwrap_err(),
            format!("config error: failed to read config: {path:?}: Is a directory (os error 21)")
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_rule_inserts_updates_preserves_other_rules_and_emits() {
        let fixture = CommandFixture::new();
        ConfigStore::new().save_rule(other_rule()).unwrap();
        assert_eq!(
            block_on(save_rule(
                rule(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Ok(())
        );
        assert_eq!(
            ConfigStore::new().list_rules().unwrap(),
            vec![rule(), other_rule()]
        );
        assert_event(&fixture, "sink-chat");
        let updated = Rule {
            name: "Renamed desk".into(),
            priority: 43,
            ..rule()
        };
        assert_eq!(
            block_on(save_rule(
                updated.clone(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Ok(())
        );
        assert_eq!(
            ConfigStore::new().list_rules().unwrap(),
            vec![updated, other_rule()]
        );
        assert_event(&fixture, "sink-chat");
    }

    #[test]
    fn save_rule_maps_write_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            block_on(save_rule(
                rule(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("config error: failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn delete_rule_removes_only_the_requested_rule_and_emits() {
        let fixture = CommandFixture::new();
        ConfigStore::new().save_rule(rule()).unwrap();
        ConfigStore::new().save_rule(other_rule()).unwrap();
        assert_eq!(
            block_on(delete_rule(
                "desk-rule".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Ok(())
        );
        assert_eq!(ConfigStore::new().list_rules().unwrap(), vec![other_rule()]);
        assert_event(&fixture, "sink-chat");
    }

    #[test]
    fn delete_rule_maps_write_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            block_on(delete_rule(
                "desk-rule".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("config error: failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn toggle_rule_persists_both_states_and_emits_each_change() {
        let fixture = CommandFixture::new();
        ConfigStore::new().save_rule(rule()).unwrap();
        for enabled in [false, true] {
            assert_eq!(
                block_on(toggle_rule(
                    "desk-rule".into(),
                    enabled,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>()
                )),
                Ok(())
            );
            assert_eq!(
                ConfigStore::new().list_rules().unwrap(),
                vec![Rule { enabled, ..rule() }]
            );
            assert_event(&fixture, "sink-chat");
        }
    }

    #[test]
    fn toggle_rule_maps_missing_rule_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(toggle_rule(
                "absent-rule".into(),
                false,
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("config error: failed to read config: rule not found: absent-rule".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    fn no_rule_simulations() -> Vec<Value> {
        [("stream-discord", "Discord · discord"), ("stream-spotify", "Spotify · spotify"), ("stream-steam", "Steam · steam"),
            ("stream-firefox", "Firefox · firefox"), ("stream-obs", "OBS · obs")].into_iter().map(|(id, label)| json!({
                "stream_id": id, "stream_label": label, "is_recent": false,
                "explanation": {"source": "no_rule", "match_reasons": ["No matching routing rule"],
                    "skipped_candidates": [], "action_status": "no_action", "fallback_applied": false}
            })).collect()
    }

    #[test]
    fn simulate_rules_returns_full_payloads_and_honors_priority_overrides_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            serde_json::to_value(
                block_on(simulate_rules(None, fixture.app.state::<AppState>())).unwrap()
            )
            .unwrap(),
            json!(no_rule_simulations())
        );
        assert!(fixture.take_graph_updates().is_empty());
        ConfigStore::new().save_rule(rule()).unwrap();
        ConfigStore::new().save_rule(other_rule()).unwrap();
        for (overrides, winner_id, winner_name, priority, target, loser_id, loser_name) in [
            (
                None,
                "desk-rule",
                "Desk audio",
                31,
                "sink-speakers",
                "other-rule",
                "Headphone audio",
            ),
            (
                Some(std::collections::HashMap::from([("other-rule".into(), 50)])),
                "other-rule",
                "Headphone audio",
                50,
                "sink-headphones",
                "desk-rule",
                "Desk audio",
            ),
        ] {
            let mut expected = no_rule_simulations();
            expected[0] = json!({"stream_id": "stream-discord", "stream_label": "Discord · discord", "is_recent": false,
                "would_target_device_id": target,
                "explanation": {"source": "authored_rule", "matched_rule_id": winner_id, "matched_rule_key": winner_name,
                    "match_reasons": ["app_name == Discord"], "action_status": "no_action", "fallback_applied": false,
                    "target_system_name": target, "target_system_names": [target],
                    "skipped_candidates": [{"rule_key": loser_name, "rule_id": loser_id, "kind": "lower_priority",
                        "reason": format!("Lower priority than {winner_name} (priority {priority})")}]
                }
            });
            let result =
                block_on(simulate_rules(overrides, fixture.app.state::<AppState>())).unwrap();
            assert_eq!(serde_json::to_value(result).unwrap(), json!(expected));
            assert!(fixture.take_graph_updates().is_empty());
        }
    }

    #[test]
    fn apply_rules_returns_success_refreshes_and_emits() {
        let fixture = CommandFixture::new();
        ConfigStore::new().save_rule(rule()).unwrap();
        ConfigStore::new()
            .set_device_alias("source-mic", "Rules refresh microphone")
            .unwrap();
        let result = block_on(apply_rules(
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": true, "message": null})
        );
        assert_event(&fixture, "sink-chat");
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.blocking_read();
        let stream = engine
            .runtime_graph()
            .streams
            .iter()
            .find(|stream| stream.id == "stream-discord")
            .unwrap();
        assert!(stream.route_explanation.is_none());
        assert_eq!(
            engine
                .runtime_graph()
                .devices
                .iter()
                .find(|device| device.id == "source-mic")
                .unwrap()
                .label,
            "Rules refresh microphone"
        );
    }

    #[test]
    fn apply_rules_maps_refresh_failure_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.use_backend(Box::new(crate::backend::stub::StubBackend::new()));
        assert_eq!(
            block_on(apply_rules(
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "pipewire adapter error: no audio backend implemented for this platform yet"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
