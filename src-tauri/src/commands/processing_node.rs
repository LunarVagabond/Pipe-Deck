use crate::core::models::{ApplyResult, PortDirection, ProcessingNode, ProcessingNodeSpecKind};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_processing_node<R: tauri::Runtime>(
    label: String,
    kind: ProcessingNodeSpecKind,
    app: tauri::AppHandle<R>,
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
pub async fn create_output_group<R: tauri::Runtime>(
    label: String,
    member_device_ids: Vec<String>,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ProcessingNode, String> {
    let mut engine = state.engine.write().await;
    let node = engine
        .create_output_group(&label, &member_device_ids)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(node)
}

#[tauri::command]
pub async fn remove_processing_node<R: tauri::Runtime>(
    id: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .remove_processing_node(&id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn connect_processing_node_port<R: tauri::Runtime>(
    node_id: String,
    direction: PortDirection,
    peer_id: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .connect_processing_node_port(&node_id, direction, &peer_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_input_gain<R: tauri::Runtime>(
    node_id: String,
    port_index: u32,
    gain_percent: u8,
    muted: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_input_gain(&node_id, port_index, gain_percent, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_volume<R: tauri::Runtime>(
    node_id: String,
    volume_percent: u8,
    muted: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_volume(&node_id, volume_percent, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_processing_node_eq_params<R: tauri::Runtime>(
    node_id: String,
    eq_sub: i32,
    eq_bass: i32,
    eq_mid: i32,
    eq_treble: i32,
    eq_air: i32,
    output_gain: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_eq_params(
            &node_id,
            eq_sub,
            eq_bass,
            eq_mid,
            eq_treble,
            eq_air,
            output_gain,
        )
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_delay_params<R: tauri::Runtime>(
    node_id: String,
    delay_ms: i32,
    feedback_percent: i32,
    feedforward_percent: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_delay_params(
            &node_id,
            delay_ms,
            feedback_percent,
            feedforward_percent,
        )
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_limiter_params<R: tauri::Runtime>(
    node_id: String,
    ceiling_db: i32,
    floor_db: i32,
    symmetric: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_limiter_params(&node_id, ceiling_db, floor_db, symmetric)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_hpf_params<R: tauri::Runtime>(
    node_id: String,
    freq_hz: i32,
    resonance_x10: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_hpf_params(&node_id, freq_hz, resonance_x10)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_reverb_params<R: tauri::Runtime>(
    node_id: String,
    mix_percent: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_reverb_params(&node_id, mix_percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_widener_params<R: tauri::Runtime>(
    node_id: String,
    width_percent: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_widener_params(&node_id, width_percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn update_processing_node_pan_params<R: tauri::Runtime>(
    node_id: String,
    balance_percent: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_pan_params(&node_id, balance_percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_processing_node_compressor_params<R: tauri::Runtime>(
    node_id: String,
    threshold_db: i32,
    ratio_x10: i32,
    attack_ms: i32,
    release_ms: i32,
    makeup_gain_db: i32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .update_processing_node_compressor_params(
            &node_id,
            threshold_db,
            ratio_x10,
            attack_ms,
            release_ms,
            makeup_gain_db,
        )
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn set_processing_node_bypassed<R: tauri::Runtime>(
    node_id: String,
    bypassed: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_processing_node_bypassed(&node_id, bypassed)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn disconnect_processing_node_port<R: tauri::Runtime>(
    node_id: String,
    direction: PortDirection,
    port_index: u32,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .disconnect_processing_node_port(&node_id, direction, port_index)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{
        connect_processing_node_port, create_output_group, create_processing_node,
        disconnect_processing_node_port, remove_processing_node, set_processing_node_bypassed,
        update_processing_node_compressor_params, update_processing_node_delay_params,
        update_processing_node_eq_params, update_processing_node_hpf_params,
        update_processing_node_input_gain, update_processing_node_limiter_params,
        update_processing_node_pan_params, update_processing_node_reverb_params,
        update_processing_node_volume, update_processing_node_widener_params,
    };
    use crate::commands::test_support::CommandFixture;
    use crate::core::models::{ApplyResult, PortDirection, ProcessingNodeSpecKind};
    use crate::AppState;
    use serde_json::{json, Value};
    use tauri::{async_runtime::block_on, Manager};

    fn assert_applied(result: ApplyResult) {
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success":true,"message":null})
        );
    }

    fn payload(slug: &str, kind: Value) -> Value {
        json!({"id":format!("processing-{slug}-control-73"),"system_name":format!("pipe-deck-proc-{slug}-control-73"),
            "label":"Control 73","kind":kind,"live":false,"bypassed":false})
    }

    async fn assert_node_event(fixture: &CommandFixture, id: &str, expected: Value) {
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.read().await;
        let node = engine
            .runtime_graph()
            .processing_nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap();
        assert_eq!(serde_json::to_value(node).unwrap(), expected);
        fixture.assert_graph_update(engine.runtime_graph());
    }

    async fn create(
        fixture: &CommandFixture,
        kind: ProcessingNodeSpecKind,
        slug: &str,
        expected_kind: Value,
    ) {
        let node = create_processing_node(
            "Control 73".into(),
            kind,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(node).unwrap(),
            payload(slug, expected_kind.clone())
        );
        assert_node_event(
            fixture,
            &format!("processing-{slug}-control-73"),
            payload(slug, expected_kind),
        )
        .await;
    }

    #[test]
    fn create_processing_node_returns_the_full_payload_and_emits() {
        let fixture = CommandFixture::new();
        block_on(create(
            &fixture,
            ProcessingNodeSpecKind::FanOut {
                volume_percent: 67,
                muted: true,
            },
            "fan_out",
            json!({"kind":"fan_out","volume_percent":67,"muted":true}),
        ));
    }

    #[test]
    fn create_processing_node_maps_duplicate_name_without_emitting() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Mixer,
                "mixer",
                json!({"kind":"mixer","input_gains_percent":[]}),
            )
            .await;
            let error = create_processing_node(
                "Control 73".into(),
                ProcessingNodeSpecKind::Mixer,
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap_err();
            assert_eq!(error,"invalid input: a processing node with this name already exists: processing-mixer-control-73");
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn create_output_group_returns_every_member_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            let node = create_output_group(
                "Control 73".into(),
                vec!["sink-speakers".into(), "sink-headphones".into()],
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            let mut expected = payload(
                "group",
                json!({"kind":"group","volume_percent":100,"muted":false}),
            );
            expected["outputs"] = json!([{"index":0,"connected_id":"sink-speakers"},{"index":1,"connected_id":"sink-headphones"}]);
            assert_eq!(serde_json::to_value(node).unwrap(), expected);
            assert_node_event(&fixture, "processing-group-control-73", expected).await;
        });
    }

    #[test]
    fn create_output_group_maps_too_few_members_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(create_output_group(
            "Control 73".into(),
            vec!["sink-speakers".into()],
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "invalid input: a group needs at least 2 members");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn remove_processing_node_removes_only_the_requested_node_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Pan {
                    balance_percent: 23,
                },
                "pan",
                json!({"kind":"pan","balance_percent":23}),
            )
            .await;
            create(
                &fixture,
                ProcessingNodeSpecKind::Mixer,
                "mixer",
                json!({"kind":"mixer","input_gains_percent":[]}),
            )
            .await;
            assert_applied(
                remove_processing_node(
                    "processing-pan-control-73".into(),
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            assert_eq!(
                serde_json::to_value(&engine.runtime_graph().processing_nodes).unwrap(),
                json!([payload(
                    "mixer",
                    json!({"kind":"mixer","input_gains_percent":[]})
                )])
            );
            fixture.assert_graph_update(engine.runtime_graph());
        });
    }

    #[test]
    fn connect_processing_node_port_connects_both_directions_and_emits() {
        for (direction, kind, slug, peer, expected_kind) in [
            (
                PortDirection::Input,
                ProcessingNodeSpecKind::Mixer,
                "mixer",
                "source-mic",
                json!({"kind":"mixer","input_gains_percent":[100]}),
            ),
            (
                PortDirection::Output,
                ProcessingNodeSpecKind::FanOut {
                    volume_percent: 71,
                    muted: false,
                },
                "fan_out",
                "sink-speakers",
                json!({"kind":"fan_out","volume_percent":71,"muted":false}),
            ),
        ] {
            let fixture = CommandFixture::new();
            block_on(async {
                let initial = if direction == PortDirection::Input {
                    json!({"kind":"mixer","input_gains_percent":[]})
                } else {
                    expected_kind.clone()
                };
                create(&fixture, kind, slug, initial).await;
                let id = format!("processing-{slug}-control-73");
                assert_applied(
                    connect_processing_node_port(
                        id.clone(),
                        direction,
                        peer.into(),
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>(),
                    )
                    .await
                    .unwrap(),
                );
                let mut expected = payload(slug, expected_kind);
                expected[if direction == PortDirection::Input {
                    "inputs"
                } else {
                    "outputs"
                }] = json!([{"index":0,"connected_id":peer}]);
                assert_node_event(&fixture, &id, expected).await;
            });
        }
    }

    #[test]
    fn update_processing_node_input_gain_changes_the_requested_input_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Mixer,
                "mixer",
                json!({"kind":"mixer","input_gains_percent":[]}),
            )
            .await;
            for (peer, gains) in [
                ("source-mic", json!([100])),
                ("sink-chat", json!([100, 100])),
            ] {
                assert_applied(
                    connect_processing_node_port(
                        "processing-mixer-control-73".into(),
                        PortDirection::Input,
                        peer.into(),
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>(),
                    )
                    .await
                    .unwrap(),
                );
                let mut expected =
                    payload("mixer", json!({"kind":"mixer","input_gains_percent":gains}));
                expected["inputs"] = if peer == "source-mic" {
                    json!([{"index":0,"connected_id":"source-mic"}])
                } else {
                    json!([{"index":0,"connected_id":"source-mic"},{"index":1,"connected_id":"sink-chat"}])
                };
                assert_node_event(&fixture, "processing-mixer-control-73", expected).await;
            }
            assert_applied(
                update_processing_node_input_gain(
                    "processing-mixer-control-73".into(),
                    1,
                    43,
                    true,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            let mut expected = payload(
                "mixer",
                json!({"kind":"mixer","input_gains_percent":[100,43]}),
            );
            expected["inputs"] = json!([{"index":0,"connected_id":"source-mic"},{"index":1,"connected_id":"sink-chat"}]);
            assert_node_event(&fixture, "processing-mixer-control-73", expected).await;
        });
    }

    #[test]
    fn update_processing_node_volume_changes_volume_and_both_mute_states_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::FanOut {
                    volume_percent: 100,
                    muted: false,
                },
                "fan_out",
                json!({"kind":"fan_out","volume_percent":100,"muted":false}),
            )
            .await;
            for (volume, muted) in [(37, true), (61, false)] {
                assert_applied(
                    update_processing_node_volume(
                        "processing-fan_out-control-73".into(),
                        volume,
                        muted,
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>(),
                    )
                    .await
                    .unwrap(),
                );
                assert_node_event(
                    &fixture,
                    "processing-fan_out-control-73",
                    payload(
                        "fan_out",
                        json!({"kind":"fan_out","volume_percent":volume,"muted":muted}),
                    ),
                )
                .await;
            }
        });
    }

    #[test]
    fn set_processing_node_bypassed_preserves_parameters_in_both_states_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(&fixture,ProcessingNodeSpecKind::Eq5Band {eq_sub:-2,eq_bass:3,eq_mid:5,eq_treble:-7,eq_air:9,output_gain:-4},"eq5band",json!({"kind":"eq5band","eq_sub":-2,"eq_bass":3,"eq_mid":5,"eq_treble":-7,"eq_air":9,"output_gain":-4})).await;
            for bypassed in [true, false] {
                assert_applied(
                    set_processing_node_bypassed(
                        "processing-eq5band-control-73".into(),
                        bypassed,
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>(),
                    )
                    .await
                    .unwrap(),
                );
                let mut expected = payload(
                    "eq5band",
                    json!({"kind":"eq5band","eq_sub":-2,"eq_bass":3,"eq_mid":5,"eq_treble":-7,"eq_air":9,"output_gain":-4}),
                );
                expected["bypassed"] = json!(bypassed);
                assert_node_event(&fixture, "processing-eq5band-control-73", expected).await;
            }
        });
    }

    #[test]
    fn disconnect_processing_node_port_removes_the_requested_port_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            let node = create_output_group(
                "Control 73".into(),
                vec!["sink-headphones".into(), "sink-speakers".into()],
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            let mut expected = payload(
                "group",
                json!({"kind":"group","volume_percent":100,"muted":false}),
            );
            expected["outputs"] = json!([{"index":0,"connected_id":"sink-headphones"},{"index":1,"connected_id":"sink-speakers"}]);
            assert_eq!(serde_json::to_value(node).unwrap(), expected);
            assert_node_event(&fixture, "processing-group-control-73", expected).await;
            assert_applied(
                disconnect_processing_node_port(
                    "processing-group-control-73".into(),
                    PortDirection::Output,
                    0,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            let mut expected = payload(
                "group",
                json!({"kind":"group","volume_percent":100,"muted":false}),
            );
            expected["outputs"] = json!([{"index":0,"connected_id":"sink-speakers"}]);
            assert_node_event(&fixture, "processing-group-control-73", expected).await;
        });
    }

    #[test]
    fn update_processing_node_eq_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(&fixture,ProcessingNodeSpecKind::Eq5Band {eq_sub:0,eq_bass:0,eq_mid:0,eq_treble:0,eq_air:0,output_gain:0},"eq5band",json!({"kind":"eq5band","eq_sub":0,"eq_bass":0,"eq_mid":0,"eq_treble":0,"eq_air":0,"output_gain":0})).await;
            assert_applied(
                update_processing_node_eq_params(
                    "processing-eq5band-control-73".into(),
                    -2,
                    3,
                    5,
                    -7,
                    9,
                    -4,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(&fixture,"processing-eq5band-control-73",payload("eq5band",json!({"kind":"eq5band","eq_sub":-2,"eq_bass":3,"eq_mid":5,"eq_treble":-7,"eq_air":9,"output_gain":-4}))).await;
        });
    }

    #[test]
    fn update_processing_node_delay_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Delay {
                    delay_ms: 0,
                    feedback_percent: 0,
                    feedforward_percent: 0,
                },
                "delay",
                json!({"kind":"delay","delay_ms":0,"feedback_percent":0,"feedforward_percent":0}),
            )
            .await;
            assert_applied(
                update_processing_node_delay_params(
                    "processing-delay-control-73".into(),
                    173,
                    29,
                    -41,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(&fixture,"processing-delay-control-73",payload("delay",json!({"kind":"delay","delay_ms":173,"feedback_percent":29,"feedforward_percent":-41}))).await;
        });
    }

    #[test]
    fn update_processing_node_limiter_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Limiter {
                    ceiling_db: 0,
                    floor_db: 0,
                    symmetric: true,
                },
                "limiter",
                json!({"kind":"limiter","ceiling_db":0,"floor_db":0,"symmetric":true}),
            )
            .await;
            assert_applied(
                update_processing_node_limiter_params(
                    "processing-limiter-control-73".into(),
                    -7,
                    -11,
                    false,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(
                &fixture,
                "processing-limiter-control-73",
                payload(
                    "limiter",
                    json!({"kind":"limiter","ceiling_db":-7,"floor_db":-11,"symmetric":false}),
                ),
            )
            .await;
        });
    }

    #[test]
    fn update_processing_node_hpf_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Hpf {
                    freq_hz: 20,
                    resonance_x10: 7,
                },
                "hpf",
                json!({"kind":"hpf","freq_hz":20,"resonance_x10":7}),
            )
            .await;
            assert_applied(
                update_processing_node_hpf_params(
                    "processing-hpf-control-73".into(),
                    347,
                    23,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(
                &fixture,
                "processing-hpf-control-73",
                payload(
                    "hpf",
                    json!({"kind":"hpf","freq_hz":347,"resonance_x10":23}),
                ),
            )
            .await;
        });
    }

    #[test]
    fn update_processing_node_reverb_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Reverb { mix_percent: 0 },
                "reverb",
                json!({"kind":"reverb","mix_percent":0}),
            )
            .await;
            assert_applied(
                update_processing_node_reverb_params(
                    "processing-reverb-control-73".into(),
                    47,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(
                &fixture,
                "processing-reverb-control-73",
                payload("reverb", json!({"kind":"reverb","mix_percent":47})),
            )
            .await;
        });
    }

    #[test]
    fn update_processing_node_widener_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Widener { width_percent: 100 },
                "widener",
                json!({"kind":"widener","width_percent":100}),
            )
            .await;
            assert_applied(
                update_processing_node_widener_params(
                    "processing-widener-control-73".into(),
                    163,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(
                &fixture,
                "processing-widener-control-73",
                payload("widener", json!({"kind":"widener","width_percent":163})),
            )
            .await;
        });
    }

    #[test]
    fn update_processing_node_pan_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(
                &fixture,
                ProcessingNodeSpecKind::Pan { balance_percent: 0 },
                "pan",
                json!({"kind":"pan","balance_percent":0}),
            )
            .await;
            assert_applied(
                update_processing_node_pan_params(
                    "processing-pan-control-73".into(),
                    -37,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(
                &fixture,
                "processing-pan-control-73",
                payload("pan", json!({"kind":"pan","balance_percent":-37})),
            )
            .await;
        });
    }

    #[test]
    fn update_processing_node_compressor_params_updates_every_parameter_and_emits() {
        let fixture = CommandFixture::new();
        block_on(async {
            create(&fixture,ProcessingNodeSpecKind::Compressor {threshold_db:0,ratio_x10:10,attack_ms:1,release_ms:50,makeup_gain_db:0},"compressor",json!({"kind":"compressor","threshold_db":0,"ratio_x10":10,"attack_ms":1,"release_ms":50,"makeup_gain_db":0})).await;
            assert_applied(
                update_processing_node_compressor_params(
                    "processing-compressor-control-73".into(),
                    -23,
                    37,
                    13,
                    271,
                    5,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>(),
                )
                .await
                .unwrap(),
            );
            assert_node_event(&fixture,"processing-compressor-control-73",payload("compressor",json!({"kind":"compressor","threshold_db":-23,"ratio_x10":37,"attack_ms":13,"release_ms":271,"makeup_gain_db":5}))).await;
        });
    }

    #[test]
    fn remove_processing_node_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(remove_processing_node(
            "absent-node-19".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn connect_processing_node_port_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(connect_processing_node_port(
            "absent-node-19".into(),
            PortDirection::Input,
            "source-mic".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn disconnect_processing_node_port_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(disconnect_processing_node_port(
            "absent-node-19".into(),
            PortDirection::Output,
            7,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_input_gain_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_input_gain(
            "absent-node-19".into(),
            3,
            43,
            true,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_volume_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_volume(
            "absent-node-19".into(),
            37,
            true,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_processing_node_bypassed_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(set_processing_node_bypassed(
            "absent-node-19".into(),
            true,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_eq_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_eq_params(
            "absent-node-19".into(),
            -2,
            3,
            5,
            -7,
            9,
            -4,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_delay_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_delay_params(
            "absent-node-19".into(),
            173,
            29,
            -41,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_limiter_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_limiter_params(
            "absent-node-19".into(),
            -7,
            -11,
            false,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_hpf_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_hpf_params(
            "absent-node-19".into(),
            347,
            23,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_reverb_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_reverb_params(
            "absent-node-19".into(),
            47,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_widener_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_widener_params(
            "absent-node-19".into(),
            163,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_pan_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_pan_params(
            "absent-node-19".into(),
            -37,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn update_processing_node_compressor_params_maps_missing_node_without_emitting() {
        let fixture = CommandFixture::new();
        let error = block_on(update_processing_node_compressor_params(
            "absent-node-19".into(),
            -23,
            37,
            13,
            271,
            5,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "not found: processing node not found: absent-node-19"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
