use crate::core::models::{StreamDirection};
use crate::pipewire::adapter::AdapterError;
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PactlSinkInput {
    pub index: u32,
    /// The real PipeWire node id backing this sink-input (pactl's `object.id`
    /// property). This is the same id `pw_dump.rs` uses to build `Stream.id`
    /// (`node-{id}`), so it's an authoritative, always-unique correlator —
    /// unlike app/media name, which two tabs of the same app can share.
    pub object_id: Option<u32>,
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
    /// See `PactlSinkInput::object_id`.
    pub object_id: Option<u32>,
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
    if let Some(object_id) = input.object_id {
        return stream.id == format!("node-{object_id}");
    }

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
    if let Some(object_id) = output.object_id {
        return stream.id == format!("node-{object_id}");
    }

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

    parse_sink_inputs_from_text(&String::from_utf8_lossy(&output.stdout))
}

fn parse_sink_inputs_from_text(text: &str) -> Vec<PactlSinkInput> {
    let mut inputs = Vec::new();
    let mut current_index = None;
    let mut current_object_id = None;
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
                        object_id: current_object_id.take(),
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
            current_object_id = None;
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
        if let Some(rest) = line.strip_prefix("object.id = ") {
            current_object_id = rest.trim_matches('"').parse().ok();
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
                object_id: current_object_id,
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
    let mut current_object_id = None;
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
                        object_id: current_object_id.take(),
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
            current_object_id = None;
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
        if let Some(rest) = line.strip_prefix("object.id = ") {
            current_object_id = rest.trim_matches('"').parse().ok();
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
                object_id: current_object_id,
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

pub(crate) fn extract_volume_percent(line: &str) -> Option<u8> {
    line.split('/')
        .nth(1)
        .and_then(|part| part.trim().strip_suffix('%'))
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::Stream;

    fn stream(id: &str, app_name: &str, media_name: Option<&str>) -> Stream {
        Stream {
            id: id.to_string(),
            app_name: app_name.to_string(),
            executable: None,
            window_class: None,
            system_name: Some(app_name.to_string()),
            direction: StreamDirection::Playback,
            current_target: None,
            current_targets: Vec::new(),
            media_name: media_name.map(str::to_string),
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: None,
        }
    }

    fn sink_input(index: u32, object_id: Option<u32>) -> PactlSinkInput {
        PactlSinkInput {
            index,
            object_id,
            application_name: "Firefox".to_string(),
            executable: None,
            node_name: Some("Firefox".to_string()),
            media_name: None,
            sink_index: Some(0),
            volume_percent: None,
            muted: None,
        }
    }

    #[test]
    fn object_id_disambiguates_identical_looking_streams() {
        // Two Firefox tabs: same app/node name, no distinguishing media name —
        // exactly the case that used to make both streams resolve to whichever
        // sink-input the name heuristic found first.
        let stream_a = stream("node-100", "Firefox", None);
        let stream_b = stream("node-102", "Firefox", None);
        let input_a = sink_input(51081, Some(100));
        let input_b = sink_input(52712, Some(102));

        assert!(stream_matches_sink_input(&stream_a, &input_a));
        assert!(!stream_matches_sink_input(&stream_a, &input_b));
        assert!(stream_matches_sink_input(&stream_b, &input_b));
        assert!(!stream_matches_sink_input(&stream_b, &input_a));
    }

    #[test]
    fn falls_back_to_name_heuristic_when_object_id_missing() {
        // Fallback heuristic only reliably disambiguates genuinely distinct
        // apps (no object.id case is a defensive floor, not the fix) — two
        // identical-looking tabs of the *same* app are exactly what object_id
        // matching above exists to handle instead.
        let mut input = sink_input(51081, None);
        input.media_name = Some("Track A".to_string());
        let matching_stream = stream("node-100", "Firefox", Some("Track A"));
        let other_stream = stream("node-102", "Chrome", Some("Track B"));

        assert!(stream_matches_sink_input(&matching_stream, &input));
        assert!(!stream_matches_sink_input(&other_stream, &input));
    }

    #[test]
    fn parses_object_id_from_sink_input_properties() {
        let text = r#"
Sink Input #51081
	Driver: PipeWire
	Client: 87
	Sink: 120
	Mute: no
	Volume: front-left: 65536 / 100% /   0.00 dB,   front-right: 65536 / 100% /   0.00 dB
	Properties:
		application.name = "Firefox"
		application.process.binary = "firefox-bin"
		node.name = "Firefox"
		media.name = "(42) CHU - Japanese Samurai Ambience"
		object.id = "100"
		object.serial = "51081"

Sink Input #52712
	Driver: PipeWire
	Client: 87
	Sink: 58
	Mute: no
	Volume: front-left: 64860 /  99% /  -0.27 dB,   front-right: 64860 /  99% /  -0.27 dB
	Properties:
		application.name = "Firefox"
		application.process.binary = "firefox-bin"
		node.name = "Firefox"
		media.name = "(42) ZEN - Japanese Samurai Ambience"
		object.id = "102"
		object.serial = "52712"
"#;

        let inputs = parse_sink_inputs_from_text(text);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].index, 51081);
        assert_eq!(inputs[0].object_id, Some(100));
        assert_eq!(inputs[0].sink_index, Some(120));
        assert_eq!(inputs[1].index, 52712);
        assert_eq!(inputs[1].object_id, Some(102));
        assert_eq!(inputs[1].sink_index, Some(58));
    }
}
