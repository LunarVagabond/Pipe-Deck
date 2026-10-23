use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn set_device_volume<R: tauri::Runtime>(
    device_id: String,
    percent: u8,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_device_volume(&device_id, percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_device_mute<R: tauri::Runtime>(
    device_id: String,
    muted: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_device_mute(&device_id, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_stream_volume<R: tauri::Runtime>(
    stream_id: String,
    percent: u8,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_stream_volume(&stream_id, percent)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_stream_mute<R: tauri::Runtime>(
    stream_id: String,
    muted: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .set_stream_mute(&stream_id, muted)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{set_device_mute, set_device_volume, set_stream_mute, set_stream_volume};
    use crate::commands::test_support::CommandFixture;
    use crate::AppState;
    use tauri::{async_runtime::block_on, Manager};

    #[test]
    fn set_device_volume_updates_the_requested_device_and_clamps() {
        let fixture = CommandFixture::new();
        block_on(async {
            for (percent, expected) in [(37, 37), (143, 100)] {
                assert_eq!(
                    set_device_volume(
                        "sink-speakers".into(),
                        percent,
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>()
                    )
                    .await,
                    Ok(())
                );
                let state = fixture.app.state::<AppState>();
                let engine = state.engine.read().await;
                let graph = engine.runtime_graph();
                assert_eq!(
                    graph
                        .devices
                        .iter()
                        .find(|device| device.id == "sink-speakers")
                        .unwrap()
                        .volume_percent,
                    Some(expected)
                );
                assert_eq!(
                    graph
                        .devices
                        .iter()
                        .find(|device| device.id == "sink-headphones")
                        .unwrap()
                        .volume_percent,
                    Some(70)
                );
                fixture.assert_graph_update(graph);
            }
        });
    }

    #[test]
    fn set_device_volume_maps_backend_error_to_string() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(set_device_volume(
                "missing-device".into(),
                37,
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("pipewire adapter error: device not found: missing-device".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_device_mute_updates_the_requested_device_in_both_directions() {
        let fixture = CommandFixture::new();
        block_on(async {
            for muted in [true, false] {
                assert_eq!(
                    set_device_mute(
                        "sink-speakers".into(),
                        muted,
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>()
                    )
                    .await,
                    Ok(())
                );
                let state = fixture.app.state::<AppState>();
                let engine = state.engine.read().await;
                let graph = engine.runtime_graph();
                assert_eq!(
                    graph
                        .devices
                        .iter()
                        .find(|device| device.id == "sink-speakers")
                        .unwrap()
                        .muted,
                    Some(muted)
                );
                assert_eq!(
                    graph
                        .devices
                        .iter()
                        .find(|device| device.id == "sink-headphones")
                        .unwrap()
                        .muted,
                    Some(false)
                );
                fixture.assert_graph_update(graph);
            }
        });
    }

    #[test]
    fn set_device_mute_maps_backend_error_to_string() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(set_device_mute(
                "missing-device".into(),
                true,
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("pipewire adapter error: device not found: missing-device".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_stream_volume_updates_the_requested_stream_and_clamps() {
        let fixture = CommandFixture::new();
        block_on(async {
            for (percent, expected) in [(23, 23), (147, 100)] {
                assert_eq!(
                    set_stream_volume(
                        "stream-spotify".into(),
                        percent,
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>()
                    )
                    .await,
                    Ok(())
                );
                let state = fixture.app.state::<AppState>();
                let engine = state.engine.read().await;
                let graph = engine.runtime_graph();
                assert_eq!(
                    graph
                        .streams
                        .iter()
                        .find(|stream| stream.id == "stream-spotify")
                        .unwrap()
                        .volume_percent,
                    Some(expected)
                );
                assert_eq!(
                    graph
                        .streams
                        .iter()
                        .find(|stream| stream.id == "stream-discord")
                        .unwrap()
                        .volume_percent,
                    None
                );
                fixture.assert_graph_update(graph);
            }
        });
    }

    #[test]
    fn set_stream_volume_maps_backend_error_to_string() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(set_stream_volume(
                "missing-stream".into(),
                23,
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("pipewire adapter error: stream not found: missing-stream".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_stream_mute_updates_the_requested_stream_in_both_directions() {
        let fixture = CommandFixture::new();
        block_on(async {
            for muted in [true, false] {
                assert_eq!(
                    set_stream_mute(
                        "stream-spotify".into(),
                        muted,
                        fixture.app.handle().clone(),
                        fixture.app.state::<AppState>()
                    )
                    .await,
                    Ok(())
                );
                let state = fixture.app.state::<AppState>();
                let engine = state.engine.read().await;
                let graph = engine.runtime_graph();
                assert_eq!(
                    graph
                        .streams
                        .iter()
                        .find(|stream| stream.id == "stream-spotify")
                        .unwrap()
                        .muted,
                    Some(muted)
                );
                assert_eq!(
                    graph
                        .streams
                        .iter()
                        .find(|stream| stream.id == "stream-discord")
                        .unwrap()
                        .muted,
                    None
                );
                fixture.assert_graph_update(graph);
            }
        });
    }

    #[test]
    fn set_stream_mute_maps_backend_error_to_string() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(set_stream_mute(
                "missing-stream".into(),
                true,
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )),
            Err("pipewire adapter error: stream not found: missing-stream".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
