use crate::core::models::{LatencyPathNode, LatencyPingResult};
use crate::AppState;
use tauri::State;

/// Measures theoretical/buffering latency along an already-resolved node
/// path (issue #223) — the frontend computes the path via its own graph
/// traversal (`collectRoutingEdges`/`findNodePath`) and hands over the
/// ordered node identities; this command only does the PipeWire-side
/// measurement, same read-only shape as `get_app_info`'s
/// `platform_audio_version` call.
#[tauri::command]
pub async fn measure_latency_ping(
    path: Vec<LatencyPathNode>,
    state: State<'_, AppState>,
) -> Result<LatencyPingResult, String> {
    state
        .engine
        .read()
        .await
        .measure_latency_ping(&path)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::CommandFixture;
    use serde_json::json;
    use tauri::{async_runtime::block_on, Manager};

    #[test]
    fn latency_ping_returns_every_hop_and_the_total_without_emitting() {
        let fixture = CommandFixture::new();
        let result = block_on(measure_latency_ping(
            vec![
                LatencyPathNode {
                    id: "stream-discord".into(),
                    system_name: Some("stream-discord".into()),
                },
                LatencyPathNode {
                    id: "sink-chat".into(),
                    system_name: None,
                },
            ],
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({
                "hops": [
                    {"id": "stream-discord", "quantum": 1024, "rate": 48000, "latency_ms": 21.333333333333332},
                    {"id": "sink-chat", "quantum": 1024, "rate": 48000, "latency_ms": 21.333333333333332}
                ], "total_latency_ms": 42.666666666666664
            })
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn latency_ping_returns_empty_and_missing_hop_payloads_without_emitting() {
        let fixture = CommandFixture::new();
        let empty = block_on(measure_latency_ping(
            vec![],
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(empty).unwrap(),
            json!({"hops": [], "total_latency_ms": 0.0})
        );
        assert!(fixture.take_graph_updates().is_empty());
        let missing = block_on(measure_latency_ping(
            vec![LatencyPathNode {
                id: "absent-node".into(),
                system_name: None,
            }],
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(missing).unwrap(),
            json!({
                "hops": [{"id": "absent-node"}]
            })
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn latency_ping_maps_backend_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.use_backend(Box::new(crate::backend::stub::StubBackend::new()));
        assert_eq!(
            block_on(measure_latency_ping(
                vec![],
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "latency measurement is not supported by this backend"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
