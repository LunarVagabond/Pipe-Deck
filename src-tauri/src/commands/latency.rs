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
