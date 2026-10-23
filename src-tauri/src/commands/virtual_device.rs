use crate::core::models::{ApplyResult, VirtualDeviceResult};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_virtual_output<R: tauri::Runtime>(
    name: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<VirtualDeviceResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .create_virtual_output(&name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn create_virtual_input<R: tauri::Runtime>(
    name: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<VirtualDeviceResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .create_virtual_input(&name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn create_virtual_multi_output<R: tauri::Runtime>(
    name: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<VirtualDeviceResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .create_virtual_multi_output(&name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn remove_virtual_device<R: tauri::Runtime>(
    system_name: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .remove_virtual_device(&system_name)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn enable_stream_mic_passthrough<R: tauri::Runtime>(
    stream_id: String,
    mic_device_id: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .enable_stream_mic_passthrough(&stream_id, &mic_device_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{
        create_virtual_input, create_virtual_multi_output, create_virtual_output,
        enable_stream_mic_passthrough, remove_virtual_device,
    };
    use crate::commands::test_support::CommandFixture;
    use crate::core::models::{DeviceDirection, SinkMode};
    use crate::AppState;
    use serde_json::json;
    use tauri::{async_runtime::block_on, Manager};

    #[test]
    fn create_virtual_output_returns_the_full_device_payload() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = create_virtual_output(
                "Studio Monitor 73".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"device_id": "virtual-studio-monitor-73", "system_name": "pipe-deck-studio-monitor-73", "label": "Studio Monitor 73", "multi": false})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let device = engine
                .runtime_graph()
                .devices
                .iter()
                .find(|device| device.id == "virtual-studio-monitor-73")
                .unwrap();
            assert_eq!(device.direction, DeviceDirection::Output);
            assert_eq!(device.sink_mode, Some(SinkMode::Single));
        });
    }

    #[test]
    fn create_virtual_output_maps_config_error_to_string() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        let error = block_on(create_virtual_output(
            "Studio Monitor 73".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "config error: failed to write config: File exists (os error 17)"
        );
    }

    #[test]
    fn create_virtual_input_returns_the_full_device_payload() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = create_virtual_input(
                "Studio Mic 29".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"device_id": "virtual-studio-mic-29", "system_name": "pipe-deck-studio-mic-29", "label": "Studio Mic 29", "multi": false})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let device = engine
                .runtime_graph()
                .devices
                .iter()
                .find(|device| device.id == "virtual-studio-mic-29")
                .unwrap();
            assert_eq!(device.direction, DeviceDirection::Input);
            assert_eq!(device.sink_mode, None);
        });
    }

    #[test]
    fn create_virtual_input_maps_config_error_to_string() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        let error = block_on(create_virtual_input(
            "Studio Mic 29".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "config error: failed to write config: File exists (os error 17)"
        );
    }

    #[test]
    fn create_virtual_multi_output_returns_the_full_device_payload() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = create_virtual_multi_output(
                "Studio Fanout 61".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"device_id": "virtual-studio-fanout-61", "system_name": "pipe-deck-studio-fanout-61", "label": "Studio Fanout 61", "multi": true})
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let device = engine
                .runtime_graph()
                .devices
                .iter()
                .find(|device| device.id == "virtual-studio-fanout-61")
                .unwrap();
            assert_eq!(device.direction, DeviceDirection::Output);
            assert_eq!(device.sink_mode, Some(SinkMode::Multi));
        });
    }

    #[test]
    fn create_virtual_multi_output_maps_config_error_to_string() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        let error = block_on(create_virtual_multi_output(
            "Studio Fanout 61".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "config error: failed to write config: File exists (os error 17)"
        );
    }

    #[test]
    fn remove_virtual_device_removes_the_requested_device() {
        let fixture = CommandFixture::new();
        block_on(async {
            create_virtual_output(
                "Disposable Monitor 73".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )
            .await
            .unwrap();
            assert_eq!(
                remove_virtual_device(
                    "pipe-deck-disposable-monitor-73".into(),
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>()
                )
                .await,
                Ok(())
            );
            let state = fixture.app.state::<AppState>();
            let engine = state.engine.read().await;
            let graph = engine.runtime_graph();
            assert!(!graph
                .devices
                .iter()
                .any(|device| device.system_name == "pipe-deck-disposable-monitor-73"));
            assert!(graph
                .devices
                .iter()
                .any(|device| device.id == "sink-speakers"));
        });
    }

    #[test]
    fn remove_virtual_device_maps_config_error_to_string() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            block_on(remove_virtual_device(
                "pipe-deck-missing-device".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("config error: failed to write config: File exists (os error 17)".into())
        );
    }

    #[test]
    fn enable_stream_mic_passthrough_adds_a_mix_without_moving_the_stream() {
        let fixture = CommandFixture::new();
        block_on(async {
            let result = enable_stream_mic_passthrough(
                "stream-discord".into(),
                "source-mic-filtered".into(),
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
            let graph = engine.runtime_graph();
            let mic = graph
                .devices
                .iter()
                .find(|device| device.id == "source-mic-filtered")
                .unwrap();
            let sources: Vec<_> = mic
                .mix_sources
                .iter()
                .map(|source| {
                    (
                        source.device_id.as_str(),
                        source.volume_percent,
                        source.muted,
                    )
                })
                .collect();
            assert_eq!(
                sources,
                vec![("source-mic", 100, false), ("sink-chat", 100, false)]
            );
            let stream = graph
                .streams
                .iter()
                .find(|stream| stream.id == "stream-discord")
                .unwrap();
            assert_eq!(stream.current_target.as_deref(), Some("sink-chat"));
        });
    }

    #[test]
    fn enable_stream_mic_passthrough_maps_not_found_error_to_string() {
        let fixture = CommandFixture::new();
        let error = block_on(enable_stream_mic_passthrough(
            "missing-stream".into(),
            "source-mic-filtered".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "not found: stream not found: missing-stream");
    }

    #[test]
    fn enable_stream_mic_passthrough_maps_invalid_input_error_to_string() {
        let fixture = CommandFixture::new();
        let error = block_on(enable_stream_mic_passthrough(
            "stream-discord".into(),
            "source-mic".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "invalid input: passthrough target must be a virtual microphone"
        );
    }
}
