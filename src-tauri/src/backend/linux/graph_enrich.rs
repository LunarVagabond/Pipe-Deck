use crate::config::ConfigStore;
use crate::core::models::{Device, DeviceDirection, DeviceKind, RuntimeGraph, Stream, StreamDirection};
use crate::backend::linux::pactl;
use crate::backend::linux::stream_match::{
    is_system_stream_name, resolve_capture_target_device_id, resolve_playback_target_device_id,
    stream_matches_pactl_capture_identity, stream_matches_pactl_input,
    stream_matches_pactl_source_output,
};
use std::collections::HashMap;
use std::process::Command;

pub fn finalize_graph(graph: &mut RuntimeGraph) {
    apply_device_aliases(&mut graph.devices);
    apply_device_levels(&mut graph.devices);
    apply_pactl_stream_levels(graph);
}

pub fn enrich_graph_from_pactl(graph: &mut RuntimeGraph) {
    merge_pactl_playback_streams(graph);
    merge_pactl_capture_streams(graph);
    apply_pactl_playback_targets(graph);
    apply_pactl_capture_targets(graph);
}

pub fn apply_pactl_playback_targets(graph: &mut RuntimeGraph) {
    let sink_names = pactl::load_sink_index_names();
    let mut updates: Vec<(String, String)> = Vec::new();

    for input in pactl::list_sink_inputs() {
        let Some(sink_index) = input.sink_index else {
            continue;
        };
        let Some(sink_name) = sink_names.get(&sink_index) else {
            continue;
        };
        let Some(target_id) = resolve_playback_target_device_id(graph, sink_name) else {
            continue;
        };
        let Some(stream_id) = graph
            .streams
            .iter()
            .find(|stream| stream_matches_pactl_input(stream, &input))
            .map(|stream| stream.id.clone())
        else {
            continue;
        };
        updates.push((stream_id, target_id));
    }

    for (stream_id, target_id) in updates {
        let Some(stream) = graph.streams.iter_mut().find(|stream| stream.id == stream_id) else {
            continue;
        };
        stream.current_target = Some(target_id);
        stream.current_targets.clear();
    }
}

pub fn apply_pactl_capture_targets(graph: &mut RuntimeGraph) {
    let source_names = pactl::load_source_index_names();
    let mut updates: Vec<(String, String)> = Vec::new();

    for output in pactl::list_source_outputs() {
        let Some(source_index) = output.source_index else {
            continue;
        };
        let Some(source_name) = source_names.get(&source_index) else {
            continue;
        };
        let Some(target_id) = resolve_capture_target_device_id(graph, source_name) else {
            continue;
        };
        let Some(stream_id) = graph
            .streams
            .iter()
            .find(|stream| stream_matches_pactl_source_output(stream, &output))
            .map(|stream| stream.id.clone())
        else {
            continue;
        };
        updates.push((stream_id, target_id));
    }

    for (stream_id, target_id) in updates {
        let Some(stream) = graph.streams.iter_mut().find(|stream| stream.id == stream_id) else {
            continue;
        };
        stream.current_target = Some(target_id);
        stream.current_targets.clear();
    }
}

/// Refresh volume/mute from pactl. Virtual pipe-deck devices are merged after pw-dump
/// enumeration, so callers must invoke this again once virtual devices are on the graph.
pub(super) fn apply_device_levels(devices: &mut [Device]) {
    apply_pactl_levels(devices);
}

pub(in crate::backend) fn apply_device_aliases(devices: &mut [Device]) {
    let aliases = ConfigStore::new().device_aliases();
    for device in devices {
        if let Some(alias) = aliases.get(&device.system_name) {
            device.label = alias.clone();
        }
    }
}

#[derive(Default)]
struct PactlEndpoint {
    volume_percent: Option<u8>,
    muted: Option<bool>,
}

fn apply_pactl_stream_levels(graph: &mut RuntimeGraph) {
    let sink_inputs: HashMap<u32, pactl::PactlSinkInput> = pactl::list_sink_inputs()
        .into_iter()
        .map(|input| (input.index, input))
        .collect();
    let source_outputs: HashMap<u32, pactl::PactlSourceOutput> = pactl::list_source_outputs()
        .into_iter()
        .map(|output| (output.index, output))
        .collect();

    for stream in &mut graph.streams {
        if let Some(rest) = stream.id.strip_prefix("pactl-sink-input-") {
            if let Ok(index) = rest.parse::<u32>() {
                if let Some(input) = sink_inputs.get(&index) {
                    stream.volume_percent = input.volume_percent;
                    stream.muted = input.muted;
                }
            }
            continue;
        }
        if let Some(rest) = stream.id.strip_prefix("pactl-source-output-") {
            if let Ok(index) = rest.parse::<u32>() {
                if let Some(output) = source_outputs.get(&index) {
                    stream.volume_percent = output.volume_percent;
                    stream.muted = output.muted;
                }
            }
        }
    }
}

fn apply_pactl_levels(devices: &mut [Device]) {
    let sink_levels = load_pactl_endpoints("sinks");
    let source_levels = load_pactl_endpoints("sources");

    for device in devices {
        let levels = match device.direction {
            DeviceDirection::Output | DeviceDirection::Duplex => sink_levels.get(&device.system_name),
            DeviceDirection::Input => source_levels.get(&device.system_name),
        };

        if let Some(levels) = levels {
            device.volume_percent = levels.volume_percent;
            device.muted = levels.muted;
        }

        if device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Output {
            let monitor_name = format!("{}.monitor", device.system_name);
            let Some(monitor) = source_levels.get(&monitor_name) else {
                continue;
            };
            if monitor.muted == Some(true) {
                device.muted = Some(true);
            }
            match (device.volume_percent, monitor.volume_percent) {
                (Some(sink), Some(monitor_volume)) => {
                    device.volume_percent = Some(sink.min(monitor_volume));
                }
                (None, Some(monitor_volume)) => {
                    device.volume_percent = Some(monitor_volume);
                }
                _ => {}
            }
        }
    }
}

fn load_pactl_endpoints(kind: &str) -> HashMap<String, PactlEndpoint> {
    let output = match Command::new("pactl").args(["list", kind]).output() {
        Ok(output) if output.status.success() => output,
        _ => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut endpoints = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current = PactlEndpoint::default();

    for line in text.lines() {
        let line = line.trim();

        if line.starts_with("Name:") {
            if let Some(name) = current_name.take() {
                endpoints.insert(name, current);
                current = PactlEndpoint::default();
            }
            current_name = Some(line["Name:".len()..].trim().to_string());
            continue;
        }

        if line.starts_with("Mute:") {
            current.muted = Some(line.contains("yes"));
            continue;
        }

        if line.starts_with("Volume:") {
            current.volume_percent = extract_volume_percent(line);
        }
    }

    if let Some(name) = current_name {
        endpoints.insert(name, current);
    }

    endpoints
}

fn extract_volume_percent(line: &str) -> Option<u8> {
    line.split('/')
        .nth(1)
        .and_then(|part| part.trim().strip_suffix('%'))
        .and_then(|value| value.trim().parse().ok())
}

fn merge_pactl_playback_streams(graph: &mut RuntimeGraph) {
    let sink_names = pactl::load_sink_index_names();
    let inputs = pactl::list_sink_inputs();

    for input in inputs {
        if let Some(index) = graph
            .streams
            .iter()
            .position(|stream| stream_matches_pactl_input(stream, &input))
        {
            // This sink-input already corresponds to a stream pw-dump
            // discovered directly (the normal case for any live app) — its
            // volume/mute never gets set otherwise, since that only happens
            // here or in the synthetic-stream fallback below.
            let stream = &mut graph.streams[index];
            stream.volume_percent = input.volume_percent;
            stream.muted = input.muted;
            continue;
        }

        graph.streams.push(Stream {
            id: format!("pactl-sink-input-{}", input.index),
            app_name: input.application_name.clone(),
            executable: input.executable.clone(),
            window_class: None,
            system_name: input.node_name.clone(),
            direction: StreamDirection::Playback,
            current_target: input
                .sink_index
                .and_then(|index| sink_names.get(&index).cloned())
                .and_then(|sink_name| resolve_playback_target_device_id(graph, &sink_name)),
            current_targets: Vec::new(),
            media_name: input.media_name.clone(),
            is_system: is_system_stream_name(&input.application_name, &input.node_name),
            volume_percent: None,
            muted: None,
            route_explanation: None,
        });
    }
}

fn merge_pactl_capture_streams(graph: &mut RuntimeGraph) {
    let source_names = pactl::load_source_index_names();

    for output in pactl::list_source_outputs() {
        let target_id = output
            .source_index
            .and_then(|index| source_names.get(&index).cloned())
            .and_then(|source_name| resolve_capture_target_device_id(graph, &source_name));

        if let Some(index) = graph.streams.iter().position(|stream| {
            stream_matches_pactl_source_output(stream, &output)
                || stream_matches_pactl_capture_identity(stream, &output)
        }) {
            let stream = &mut graph.streams[index];
            if stream.direction != StreamDirection::Capture {
                stream.direction = StreamDirection::Capture;
            }
            if let Some(target_id) = target_id {
                stream.current_target = Some(target_id);
                stream.current_targets.clear();
            }
            // Same fix as merge_pactl_playback_streams: this source-output
            // already corresponds to a stream pw-dump discovered directly,
            // so its volume/mute never gets set otherwise.
            stream.volume_percent = output.volume_percent;
            stream.muted = output.muted;
            continue;
        }

        graph.streams.push(Stream {
            id: format!("pactl-source-output-{}", output.index),
            app_name: output.application_name.clone(),
            executable: output.executable.clone(),
            window_class: None,
            system_name: output.node_name.clone(),
            direction: StreamDirection::Capture,
            current_target: target_id,
            current_targets: Vec::new(),
            media_name: output.media_name.clone(),
            is_system: is_system_stream_name(&output.application_name, &output.node_name),
            volume_percent: None,
            muted: None,
            route_explanation: None,
        });
    }
}
