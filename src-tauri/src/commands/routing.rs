use crate::core::models::ApplyResult;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn set_stream_targets<R: tauri::Runtime>(
    stream_id: String,
    target_device_ids: Vec<String>,
    app: tauri::AppHandle<R>,
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
pub async fn set_stream_target<R: tauri::Runtime>(
    stream_id: String,
    target_device_id: String,
    app: tauri::AppHandle<R>,
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
pub async fn clear_stream_target<R: tauri::Runtime>(
    stream_id: String,
    previous_target_device_id: String,
    app: tauri::AppHandle<R>,
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
pub async fn undo_last_routing<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
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
    use crate::AppState;
    use serde_json::json;
    use tauri::{async_runtime::block_on, Manager};

    #[test]
    fn set_stream_targets_routes_to_the_primary_target() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = set_stream_targets(
                "stream-discord".into(),
                vec!["sink-speakers".into(), "sink-headphones".into()],
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-speakers"));
            fixture.assert_graph_update(engine.runtime_graph());
        });
    }

    #[test]
    fn set_stream_targets_returns_a_failed_payload_for_no_targets() {
        let fixture = CommandFixture::new();
        let result = block_on(set_stream_targets(
            "stream-discord".into(),
            vec![],
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "at least one target is required"})
        );
        fixture.assert_graph_update(
            fixture
                .app
                .state::<AppState>()
                .engine
                .blocking_read()
                .runtime_graph(),
        );
    }

    #[test]
    fn set_stream_targets_maps_backend_failure_into_the_payload() {
        let fixture = CommandFixture::new();
        let result = block_on(set_stream_targets(
            "stream-discord".into(),
            vec!["missing-target".into()],
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "routing error: target device not found: missing-target"})
        );
        fixture.assert_graph_update(
            fixture
                .app
                .state::<AppState>()
                .engine
                .blocking_read()
                .runtime_graph(),
        );
    }

    #[test]
    fn set_stream_target_routes_the_requested_stream() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = set_stream_target(
                "stream-spotify".into(),
                "sink-headphones".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-spotify")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-headphones"));
            fixture.assert_graph_update(engine.runtime_graph());
        });
    }

    #[test]
    fn set_stream_target_maps_backend_failure_into_the_payload() {
        let fixture = CommandFixture::new();
        let result = block_on(set_stream_target(
            "missing-stream".into(),
            "sink-speakers".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "routing error: stream not found: missing-stream"})
        );
        fixture.assert_graph_update(
            fixture
                .app
                .state::<AppState>()
                .engine
                .blocking_read()
                .runtime_graph(),
        );
    }

    #[test]
    fn clear_stream_target_removes_the_route() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = clear_stream_target(
                "stream-discord".into(),
                "sink-chat".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target, None);
            fixture.assert_graph_update(engine.runtime_graph());
        });
    }

    #[test]
    fn clear_stream_target_maps_engine_routing_error_to_string() {
        let fixture = CommandFixture::new();
        let error = block_on(clear_stream_target(
            "missing-stream".into(),
            "sink-chat".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "routing error: stream not found: missing-stream");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn undo_last_routing_restores_the_previous_target() {
        let fixture = CommandFixture::new();
        block_on(async {
            let routed = set_stream_target(
                "stream-discord".into(),
                "sink-speakers".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert!(routed.success);
            fixture.assert_graph_update(
                fixture
                    .app
                    .state::<AppState>()
                    .engine
                    .read()
                    .await
                    .runtime_graph(),
            );
            let result = undo_last_routing(
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"success": true, "message": null})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let stream = engine
                .runtime_graph()
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-chat"));
            fixture.assert_graph_update(engine.runtime_graph());
        });
    }

    #[test]
    fn undo_last_routing_returns_a_failed_payload_without_history() {
        let fixture = CommandFixture::new();
        let result = block_on(undo_last_routing(
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": false, "message": "nothing to undo"})
        );
        fixture.assert_graph_update(
            fixture
                .app
                .state::<AppState>()
                .engine
                .blocking_read()
                .runtime_graph(),
        );
    }

    #[test]
    fn can_undo_routing_reports_no_history() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(can_undo_routing(fixture.app.state::<AppState>())),
            Ok(false)
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn can_undo_routing_reports_available_history_and_its_consumption() {
        let fixture = CommandFixture::new();
        block_on(async {
            let routed = set_stream_target(
                "stream-discord".into(),
                "sink-speakers".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert!(routed.success);
            fixture.assert_graph_update(
                fixture
                    .app
                    .state::<AppState>()
                    .engine
                    .read()
                    .await
                    .runtime_graph(),
            );
            assert_eq!(
                can_undo_routing(fixture.app.state::<AppState>()).await,
                Ok(true)
            );
            assert!(fixture.take_graph_updates().is_empty());
            let undone = undo_last_routing(
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert!(undone.success);
            fixture.assert_graph_update(
                fixture
                    .app
                    .state::<AppState>()
                    .engine
                    .read()
                    .await
                    .runtime_graph(),
            );
            assert_eq!(
                can_undo_routing(fixture.app.state::<AppState>()).await,
                Ok(false)
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn get_last_error_reports_no_error() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(get_last_error(fixture.app.state::<AppState>())),
            Ok(None)
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_last_error_reports_routing_failure_and_its_clearance() {
        let fixture = CommandFixture::new();
        block_on(async {
            let failed = set_stream_target(
                "stream-discord".into(),
                "missing-target".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert!(!failed.success);
            fixture.assert_graph_update(
                fixture
                    .app
                    .state::<AppState>()
                    .engine
                    .read()
                    .await
                    .runtime_graph(),
            );
            assert_eq!(
                get_last_error(fixture.app.state::<AppState>()).await,
                Ok(Some(
                    "routing error: target device not found: missing-target".into()
                ))
            );
            assert!(fixture.take_graph_updates().is_empty());
            let routed = set_stream_target(
                "stream-discord".into(),
                "sink-speakers".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert!(routed.success);
            fixture.assert_graph_update(
                fixture
                    .app
                    .state::<AppState>()
                    .engine
                    .read()
                    .await
                    .runtime_graph(),
            );
            assert_eq!(
                get_last_error(fixture.app.state::<AppState>()).await,
                Ok(None)
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }
}
