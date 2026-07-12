use crate::core::models::{StreamDirection};
use crate::pipewire::adapter::AdapterError;
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PactlSinkInput {
    pub index: u32,
    pub application_name: String,
    pub executable: Option<String>,
    pub node_name: Option<String>,
    pub media_name: Option<String>,
    pub sink_index: Option<u32>,
    pub volume_percent: Option<u8>,
    pub muted: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct PactlSourceOutput {
    pub index: u32,
    pub application_name: String,
    pub executable: Option<String>,
    pub node_name: Option<String>,
    pub media_name: Option<String>,
    pub source_index: Option<u32>,
    pub volume_percent: Option<u8>,
    pub muted: Option<bool>,
}

pub fn list_sink_inputs() -> Vec<PactlSinkInput> {
    parse_sink_inputs()
}

pub fn list_source_outputs() -> Vec<PactlSourceOutput> {
    parse_source_outputs()
}

pub fn load_sink_index_names() -> HashMap<u32, String> {
    load_short_index_names("sinks")
}

pub fn load_source_index_names() -> HashMap<u32, String> {
    load_short_index_names("sources")
}

fn load_short_index_names(kind: &str) -> HashMap<u32, String> {
    let output = match Command::new("pactl").args(["list", kind, "short"]).output() {
        Ok(output) if output.status.success() => output,
        _ => return HashMap::new(),
    };

    let mut names = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.split_whitespace();
        let Some(index) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        names.insert(index, name.to_string());
    }
    names
}

pub fn stream_matches_sink_input(stream: &crate::core::models::Stream, input: &PactlSinkInput) -> bool {
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

pub fn stream_matches_source_output(
    stream: &crate::core::models::Stream,
    output: &PactlSourceOutput,
) -> bool {
    if stream.id == format!("pactl-source-output-{}", output.index) {
        return true;
    }

    if stream.direction != StreamDirection::Capture {
        return false;
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

pub(crate) fn find_sink_input_index(_graph: &crate::core::models::RuntimeGraph, stream: &crate::core::models::Stream) -> Result<u32, AdapterError> {
    for input in list_sink_inputs() {
        if stream_matches_sink_input(stream, &input) {
            return Ok(input.index);
        }
    }

    Err(AdapterError::Message(format!(
        "could not find pactl sink-input for stream {}",
        stream.app_name
    )))
}

pub(crate) fn find_source_output_index(
    _graph: &crate::core::models::RuntimeGraph,
    stream: &crate::core::models::Stream,
) -> Result<u32, AdapterError> {
    for output in list_source_outputs() {
        if stream_matches_source_output(stream, &output) {
            return Ok(output.index);
        }
    }

    Err(AdapterError::Message(format!(
        "could not find pactl source-output for stream {}",
        stream.app_name
    )))
}

fn parse_sink_inputs() -> Vec<PactlSinkInput> {
    let output = match Command::new("pactl").args(["list", "sink-inputs"]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut inputs = Vec::new();
    let mut current_index = None;
    let mut current_app = None;
    let mut current_executable = None;
    let mut current_node = None;
    let mut current_media = None;
    let mut current_sink = None;
    let mut current_volume = None;
    let mut current_muted = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Sink Input #") {
            if let Some(index) = current_index.take() {
                if let Some(application_name) = current_app.take() {
                    inputs.push(PactlSinkInput {
                        index,
                        application_name,
                        executable: current_executable.take(),
                        node_name: current_node.take(),
                        media_name: current_media.take(),
                        sink_index: current_sink.take(),
                        volume_percent: current_volume.take(),
                        muted: current_muted.take(),
                    });
                }
            }
            current_index = rest.parse().ok();
            current_executable = None;
            current_volume = None;
            current_muted = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("application.name = ") {
            current_app = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("application.process.binary = ") {
            current_executable = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("node.name = ") {
            current_node = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("media.name = ") {
            current_media = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Sink: ") {
            current_sink = rest.trim().parse().ok();
            continue;
        }
        if line.starts_with("Volume:") {
            current_volume = extract_volume_percent(line);
            continue;
        }
        if line.starts_with("Mute:") {
            current_muted = Some(line.contains("yes"));
        }
    }

    if let Some(index) = current_index {
        if let Some(application_name) = current_app {
            inputs.push(PactlSinkInput {
                index,
                application_name,
                executable: current_executable,
                node_name: current_node,
                media_name: current_media,
                sink_index: current_sink,
                volume_percent: current_volume,
                muted: current_muted,
            });
        }
    }

    inputs
}

fn parse_source_outputs() -> Vec<PactlSourceOutput> {
    let output = match Command::new("pactl").args(["list", "source-outputs"]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut outputs = Vec::new();
    let mut current_index = None;
    let mut current_app = None;
    let mut current_executable = None;
    let mut current_node = None;
    let mut current_media = None;
    let mut current_source = None;
    let mut current_volume = None;
    let mut current_muted = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Source Output #") {
            if let Some(index) = current_index.take() {
                if let Some(application_name) = current_app.take() {
                    outputs.push(PactlSourceOutput {
                        index,
                        application_name,
                        executable: current_executable.take(),
                        node_name: current_node.take(),
                        media_name: current_media.take(),
                        source_index: current_source.take(),
                        volume_percent: current_volume.take(),
                        muted: current_muted.take(),
                    });
                }
            }
            current_index = rest.parse().ok();
            current_executable = None;
            current_volume = None;
            current_muted = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("application.name = ") {
            current_app = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("application.process.binary = ") {
            current_executable = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("node.name = ") {
            current_node = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("media.name = ") {
            current_media = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Source: ") {
            current_source = rest.trim().parse().ok();
            continue;
        }
        if line.starts_with("Volume:") {
            current_volume = extract_volume_percent(line);
            continue;
        }
        if line.starts_with("Mute:") {
            current_muted = Some(line.contains("yes"));
        }
    }

    if let Some(index) = current_index {
        if let Some(application_name) = current_app {
            outputs.push(PactlSourceOutput {
                index,
                application_name,
                executable: current_executable,
                node_name: current_node,
                media_name: current_media,
                source_index: current_source,
                volume_percent: current_volume,
                muted: current_muted,
            });
        }
    }

    outputs
}

fn extract_volume_percent(line: &str) -> Option<u8> {
    line.split('/')
        .nth(1)
        .and_then(|part| part.trim().strip_suffix('%'))
        .and_then(|value| value.trim().parse().ok())
}
