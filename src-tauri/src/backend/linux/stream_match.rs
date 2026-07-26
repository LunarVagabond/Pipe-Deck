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
