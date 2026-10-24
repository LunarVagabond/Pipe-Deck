use crate::core::models::RuntimeGraph;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_runtime_graph(state: State<'_, AppState>) -> Result<RuntimeGraph, String> {
    let mut engine = state.engine.write().await;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    Ok(engine.runtime_graph().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::CommandFixture;
    use crate::config::ConfigStore;
    use tauri::{async_runtime::block_on, Manager};

    #[test]
    fn get_runtime_graph_refreshes_and_returns_the_entire_snapshot_without_emitting() {
        let fixture = CommandFixture::new();
        let mut expected = fixture
            .app
            .state::<AppState>()
            .engine
            .blocking_read()
            .runtime_graph()
            .clone();
        let mic = expected
            .devices
            .iter_mut()
            .find(|device| device.id == "source-mic")
            .unwrap();
        assert_eq!(mic.label, "Microphone");
        mic.label = "Desk microphone".into();
        for stream in &mut expected.streams {
            stream.route_explanation = None;
        }
        ConfigStore::new()
            .set_device_alias("source-mic", "Desk microphone")
            .unwrap();
        let before = chrono::Utc::now().timestamp() as u64;
        let result = block_on(get_runtime_graph(fixture.app.state::<AppState>())).unwrap();
        let after = chrono::Utc::now().timestamp() as u64;
        assert_eq!(
            result.recent_stream_identities.len(),
            expected.recent_stream_identities.len()
        );
        for (actual, expected) in result
            .recent_stream_identities
            .iter()
            .zip(&mut expected.recent_stream_identities)
        {
            assert!(actual.last_seen_secs >= before && actual.last_seen_secs <= after);
            expected.last_seen_secs = actual.last_seen_secs;
        }
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_runtime_graph_maps_backend_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.use_backend(Box::new(crate::backend::stub::StubBackend::new()));
        assert_eq!(
            block_on(get_runtime_graph(fixture.app.state::<AppState>())).unwrap_err(),
            "pipewire adapter error: no audio backend implemented for this platform yet"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
