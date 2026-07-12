use crate::core::models::{Stream, StreamDirection};
use crate::core::stream_identity::is_internal_audio_client;
use crate::pipewire::pactl;

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
    if stream.id == format!("pactl-source-output-{}", output.index) {
        return true;
    }

    if stream.direction != StreamDirection::Capture {
        return false;
    }

    stream_matches_pactl_capture_identity(stream, output)
}

pub fn stream_matches_pactl_capture_identity(
    stream: &Stream,
    output: &pactl::PactlSourceOutput,
) -> bool {
    if let Some(system_name) = &stream.system_name {
        if output
            .node_name
            .as_deref()
            .is_some_and(|node_name| node_name == system_name)
        {
            return true;
        }
    }

    if stream.app_name != output.application_name {
        if stream
            .executable
            .as_deref()
            .is_none_or(|executable| executable != output.application_name)
        {
            return false;
        }
    }

    match (&stream.media_name, &output.media_name) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

pub fn stream_matches_pactl_input(stream: &Stream, input: &pactl::PactlSinkInput) -> bool {
    if stream.id == format!("pactl-sink-input-{}", input.index) {
        return true;
    }

    if stream.direction != StreamDirection::Playback {
        return false;
    }

    if let Some(system_name) = &stream.system_name {
        if input
            .node_name
            .as_deref()
            .is_some_and(|node_name| node_name == system_name)
        {
            return true;
        }
    }

    if stream.app_name != input.application_name {
        if stream
            .executable
            .as_deref()
            .is_none_or(|executable| executable != input.application_name)
        {
            return false;
        }
    }

    match (&stream.media_name, &input.media_name) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}
