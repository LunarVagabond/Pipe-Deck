use crate::core::models::ApplyResult;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn set_stream_targets(
    stream_id: String,
    target_device_ids: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_stream_targets(&stream_id, &target_device_ids)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn set_stream_target(
    stream_id: String,
    target_device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .set_stream_target(&stream_id, &target_device_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn clear_stream_target(
    stream_id: String,
    previous_target_device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .clear_stream_target(&stream_id, Some(&previous_target_device_id))
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn undo_last_routing(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .undo_last_routing()
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn can_undo_routing(state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.engine.read().await;
    Ok(engine.can_undo_routing())
}

#[tauri::command]
pub async fn get_last_error(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let engine = state.engine.read().await;
    Ok(engine.last_error().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::{
        can_undo_routing, clear_stream_target, get_last_error, set_stream_target,
        set_stream_targets, undo_last_routing,
    };
    use crate::commands::test_support::CommandFixture;
    use serde_json::json;
    use tauri::async_runtime::block_on;

    #[test]
    fn set_stream_targets_routes_to_the_primary_target() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = set_stream_targets(
                "stream-discord".into(),
                vec!["sink-speakers".into(), "sink-headphones".into()],
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.state();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-speakers"));
        });
    }

    #[test]
    fn set_stream_targets_returns_a_failed_payload_for_no_targets() {
        let fixture = CommandFixture::new();
        let result = block_on(set_stream_targets(
            "stream-discord".into(),
            vec![],
            fixture.app.clone(),
            fixture.state(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "at least one target is required"})
        );
    }

    #[test]
    fn set_stream_targets_maps_backend_failure_into_the_payload() {
        let fixture = CommandFixture::new();
        let result = block_on(set_stream_targets(
            "stream-discord".into(),
            vec!["missing-target".into()],
            fixture.app.clone(),
            fixture.state(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "routing error: target device not found: missing-target"})
        );
    }

    #[test]
    fn set_stream_target_routes_the_requested_stream() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = set_stream_target(
                "stream-spotify".into(),
                "sink-headphones".into(),
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.state();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-spotify")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-headphones"));
        });
    }

    #[test]
    fn set_stream_target_maps_backend_failure_into_the_payload() {
        let fixture = CommandFixture::new();
        let result = block_on(set_stream_target(
            "missing-stream".into(),
            "sink-speakers".into(),
            fixture.app.clone(),
            fixture.state(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "routing error: stream not found: missing-stream"})
        );
    }

    #[test]
    fn clear_stream_target_removes_the_route() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = clear_stream_target(
                "stream-discord".into(),
                "sink-chat".into(),
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.state();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target, None);
        });
    }

    #[test]
    fn clear_stream_target_maps_engine_routing_error_to_string() {
        let fixture = CommandFixture::new();
        let error = block_on(clear_stream_target(
            "missing-stream".into(),
            "sink-chat".into(),
            fixture.app.clone(),
            fixture.state(),
        ))
        .unwrap_err();
        assert_eq!(error, "routing error: stream not found: missing-stream");
    }

    #[test]
    fn undo_last_routing_restores_the_previous_target() {
        let fixture = CommandFixture::new();
        block_on(async {
            let routed = set_stream_target(
                "stream-discord".into(),
                "sink-speakers".into(),
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert!(routed.success);
            let result = undo_last_routing(fixture.app.clone(), fixture.state())
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.state();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-chat"));
        });
    }

    #[test]
    fn undo_last_routing_returns_a_failed_payload_without_history() {
        let fixture = CommandFixture::new();
        let result = block_on(undo_last_routing(fixture.app.clone(), fixture.state())).unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "nothing to undo"})
        );
    }

    #[test]
    fn can_undo_routing_reports_no_history() {
        let fixture = CommandFixture::new();
        assert_eq!(block_on(can_undo_routing(fixture.state())), Ok(false));
    }

    #[test]
    fn can_undo_routing_reports_available_history_and_its_consumption() {
        let fixture = CommandFixture::new();
        block_on(async {
            let routed = set_stream_target(
                "stream-discord".into(),
                "sink-speakers".into(),
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert!(routed.success);
            assert_eq!(can_undo_routing(fixture.state()).await, Ok(true));
            let undone = undo_last_routing(fixture.app.clone(), fixture.state())
                .await
                .unwrap();
            assert!(undone.success);
            assert_eq!(can_undo_routing(fixture.state()).await, Ok(false));
        });
    }

    #[test]
    fn get_last_error_reports_no_error() {
        let fixture = CommandFixture::new();
        assert_eq!(block_on(get_last_error(fixture.state())), Ok(None));
    }

    #[test]
    fn get_last_error_reports_routing_failure_and_its_clearance() {
        let fixture = CommandFixture::new();
        block_on(async {
            let failed = set_stream_target(
                "stream-discord".into(),
                "missing-target".into(),
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert!(!failed.success);
            assert_eq!(
                get_last_error(fixture.state()).await,
                Ok(Some(
                    "routing error: target device not found: missing-target".into()
                ))
            );
            let routed = set_stream_target(
                "stream-discord".into(),
                "sink-speakers".into(),
                fixture.app.clone(),
                fixture.state(),
            )
            .await
            .unwrap();
            assert!(routed.success);
            assert_eq!(get_last_error(fixture.state()).await, Ok(None));
        });
    }
}
