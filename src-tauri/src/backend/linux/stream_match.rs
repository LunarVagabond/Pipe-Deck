use crate::core::models::Stream;
use crate::core::stream_identity::is_internal_audio_client;
use crate::backend::linux::pactl;

pub fn is_system_stream_name(application_name: &str, node_name: &Option<String>) -> bool {
    let node_name = node_name.as_deref().unwrap_or_default();
    is_internal_audio_client(application_name) || is_internal_audio_client(node_name)
}

pub fn resolve_capture_target_device_id(
    graph: &crate::core::models::RuntimeGraph,
    source_system_name: &str,
) -> Option<String> {
    graph
        .devices
        .iter()
        .find(|device| device.system_name == source_system_name)
        .map(|device| device.id.clone())
}

pub fn resolve_playback_target_device_id(
    graph: &crate::core::models::RuntimeGraph,
    sink_system_name: &str,
) -> Option<String> {
    use crate::core::models::DeviceDirection;

    if let Some(device) = graph
        .devices
        .iter()
        .find(|device| device.system_name == sink_system_name)
    {
        return Some(device.id.clone());
    }

    // A processing node's stream input connects directly onto its own
    // backing sink for every kind except Mixer (which uses a per-input
    // `pipe-deck-feed-*` indirection, handled by the fallback below) —
    // Fan-out and Group both route a stream's sink-input straight onto
    // `pipe-deck-proc-{kind}-{slug}` itself (PD-034: "this is exactly what
    // a Fan-out already exists to do"). Without this check, a stream routed
    // into a Fan-out/Group looks unrouted after any refresh that re-derives
    // `current_target` from live pactl state (e.g. an app restart) — worse,
    // it then silently keeps whatever the raw pactl/pw-dump baseline
    // guessed before this enrichment pass ran (commonly the system default
    // sink), while the processing node's own port bookkeeping still
    // correctly shows it connected — "already connected" on a re-drag, but
    // visually pointing at the wrong node entirely.
    if let Some(node) = graph.processing_nodes.iter().find(|node| node.system_name == sink_system_name) {
        return Some(node.id.clone());
    }

    let slug = sink_system_name.strip_prefix("pipe-deck-feed-")?;

    // A Mixer processing node's own per-input feed sink (PD-032 generalizes
    // the mix-pair mechanism beyond virtual-input devices) — checked before
    // the virtual-input device fallback below, since a Mixer's system_name
    // (`pipe-deck-proc-mixer-{slug}`) can't be reconstructed by that
    // fallback's flat `pipe-deck-{slug}` assumption. Without this, a stream
    // moved into a Mixer's feed sink never resolves `current_target` back to
    // the Mixer at all — it looks unrouted to every downstream consumer
    // (manual-override detection, the routing-rules reconciler), so a saved
    // rule keeps reasserting the stream's old target and silently moves it
    // right back out of the Mixer's gain-controlled feed sink.
    if let Some(node) = graph.processing_nodes.iter().find(|node| {
        matches!(node.kind, crate::core::models::ProcessingNodeKind::Mixer { .. })
            && slug.starts_with(&format!(
                "{}-",
                node.system_name.strip_prefix("pipe-deck-").unwrap_or(&node.system_name)
            ))
    }) {
        return Some(node.id.clone());
    }

    let virtual_input_name = format!("pipe-deck-{slug}");
    graph
        .devices
        .iter()
        .find(|device| {
            device.system_name == virtual_input_name && device.direction == DeviceDirection::Input
        })
        .map(|device| device.id.clone())
}

pub fn stream_matches_pactl_source_output(stream: &Stream, output: &pactl::PactlSourceOutput) -> bool {
    pactl::stream_matches_source_output(stream, output)
}

pub fn stream_matches_pactl_capture_identity(
    stream: &Stream,
    output: &pactl::PactlSourceOutput,
) -> bool {
    if let Some(object_id) = output.object_id {
        return stream.id == format!("node-{object_id}");
    }

    if let Some(system_name) = &stream.system_name {
        if output
            .node_name
            .as_deref()
            .is_some_and(|node_name| node_name == system_name)
        {
            return true;
        }
    }

    if stream.app_name != output.application_name
        && stream
            .executable
            .as_deref()
            .is_none_or(|executable| executable != output.application_name)
    {
        return false;
    }

    match (&stream.media_name, &output.media_name) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

pub fn stream_matches_pactl_input(stream: &Stream, input: &pactl::PactlSinkInput) -> bool {
    pactl::stream_matches_sink_input(stream, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{ProcessingNode, ProcessingNodeKind, RuntimeGraph};

    fn group_node() -> ProcessingNode {
        ProcessingNode {
            id: "processing-group-test-group".into(),
            label: "Test Group".into(),
            kind: ProcessingNodeKind::Group { volume_percent: 100, muted: false },
            system_name: "pipe-deck-proc-group-test-group".into(),
            bypassed: false,
            live: true,
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    #[test]
    fn resolves_a_stream_moved_directly_onto_a_groups_own_sink() {
        let graph = RuntimeGraph { processing_nodes: vec![group_node()], ..Default::default() };
        let target = resolve_playback_target_device_id(&graph, "pipe-deck-proc-group-test-group");
        assert_eq!(target.as_deref(), Some("processing-group-test-group"));
    }

    #[test]
    fn resolves_a_stream_moved_directly_onto_a_fan_outs_own_sink() {
        let mut node = group_node();
        node.id = "processing-fan_out-stream-fan-out".into();
        node.kind = ProcessingNodeKind::FanOut { volume_percent: 100, muted: false };
        node.system_name = "pipe-deck-proc-fan_out-stream-fan-out".into();
        let graph = RuntimeGraph { processing_nodes: vec![node], ..Default::default() };
        let target = resolve_playback_target_device_id(&graph, "pipe-deck-proc-fan_out-stream-fan-out");
        assert_eq!(target.as_deref(), Some("processing-fan_out-stream-fan-out"));
    }

    #[test]
    fn returns_none_for_a_sink_name_matching_no_device_or_processing_node() {
        let graph = RuntimeGraph { processing_nodes: vec![group_node()], ..Default::default() };
        let target = resolve_playback_target_device_id(&graph, "alsa_output.some-unrelated-sink");
        assert_eq!(target, None);
    }
}
