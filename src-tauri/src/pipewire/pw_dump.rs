use crate::core::models::{
    Device, DeviceDirection, DeviceKind, Link, RuntimeGraph, Stream, StreamDirection,
};
use crate::core::stream_identity::{
    is_internal_audio_client, parse_stream_identity, parse_window_class,
};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::graph_enrich;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct PwDumpObject {
    pub id: u32,
    #[serde(rename = "type")]
    pub object_type: String,
    pub info: Option<Value>,
}

pub fn run_snapshot() -> Result<Vec<u8>, AdapterError> {
    let output = Command::new("timeout")
        .args(["5", "pw-dump", "-N"])
        .output()
        .or_else(|_| Command::new("pw-dump").arg("-N").output())
        .map_err(|error| AdapterError::Message(format!("failed to run pw-dump: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AdapterError::Message(format!("pw-dump failed: {stderr}")));
    }

    Ok(output.stdout)
}

pub fn normalize(objects: &[PwDumpObject]) -> RuntimeGraph {
    let mut devices = Vec::new();
    let mut streams = Vec::new();
    let mut raw_links: Vec<(u32, u32, u32)> = Vec::new();

    for object in objects {
        if object.object_type.ends_with("Interface:Node") {
            let Some(props) = object_props(&object.info) else {
                continue;
            };

            let media_class = prop_str(props, "media.class");
            if should_skip_media_class(&media_class) {
                continue;
            }

            let node_name = prop_str(props, "node.name");
            if node_name.contains(".monitor")
                || node_name.starts_with("pipe-deck-feed-")
                || node_name.starts_with("pipe-deck-")
            {
                continue;
            }

            let id = node_id(object.id);

            if media_class.contains("Stream/Output") {
                let (app_name, executable) = parse_stream_identity(props);
                let window_class = parse_window_class(props);
                streams.push(Stream {
                    id: id.clone(),
                    app_name,
                    executable,
                    window_class,
                    system_name: if node_name.is_empty() {
                        None
                    } else {
                        Some(node_name.clone())
                    },
                    direction: StreamDirection::Playback,
                    current_target: None,
                    current_targets: Vec::new(),
                    media_name: stream_media_name(props),
                    is_system: is_system_stream(props),
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                });
                continue;
            }

            if media_class.contains("Stream/Input") {
                let (app_name, executable) = parse_stream_identity(props);
                let window_class = parse_window_class(props);
                streams.push(Stream {
                    id: id.clone(),
                    app_name,
                    executable,
                    window_class,
                    system_name: if node_name.is_empty() {
                        None
                    } else {
                        Some(node_name.clone())
                    },
                    direction: StreamDirection::Capture,
                    current_target: None,
                    current_targets: Vec::new(),
                    media_name: stream_media_name(props),
                    is_system: is_system_stream(props),
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                });
                continue;
            }

            if media_class == "Audio/Sink" {
                devices.push(Device {
                    id: id.clone(),
                    system_name: node_name.clone(),
                    label: device_label(props, &media_class),
                    kind: if is_hardware_audio_endpoint(props) {
                        DeviceKind::Physical
                    } else {
                        DeviceKind::Virtual
                    },
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                });
                continue;
            }

            if is_source_media_class(&media_class) {
                devices.push(Device {
                    id,
                    system_name: node_name,
                    label: device_label(props, &media_class),
                    kind: if is_hardware_audio_endpoint(props) {
                        DeviceKind::Physical
                    } else if is_virtual_device(props) {
                        DeviceKind::Virtual
                    } else {
                        DeviceKind::Physical
                    },
                    direction: DeviceDirection::Input,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                });
            }
        }

        if object.object_type.ends_with("Interface:Link") {
            let Some(info) = &object.info else {
                continue;
            };
            let output_id = info
                .get("output-node-id")
                .or_else(|| info.pointer("/props/link.output.node"))
                .and_then(value_as_u32);
            let input_id = info
                .get("input-node-id")
                .or_else(|| info.pointer("/props/link.input.node"))
                .and_then(value_as_u32);

            if let (Some(output_id), Some(input_id)) = (output_id, input_id) {
                raw_links.push((object.id, output_id, input_id));
            }
        }
    }

    let stream_ids: HashSet<String> = streams.iter().map(|stream| stream.id.clone()).collect();
    let device_ids: HashSet<String> = devices.iter().map(|device| device.id.clone()).collect();
    let known_ids: HashSet<String> = stream_ids.union(&device_ids).cloned().collect();

    let mut stream_targets: HashMap<String, String> = HashMap::new();
    let mut links = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for (link_id, output_id, input_id) in raw_links {
        let source_id = node_id(output_id);
        let target_id = node_id(input_id);

        if !known_ids.contains(&source_id) || !known_ids.contains(&target_id) {
            continue;
        }

        if !seen_pairs.insert((source_id.clone(), target_id.clone())) {
            continue;
        }

        if stream_ids.contains(&source_id) {
            stream_targets.insert(source_id.clone(), target_id.clone());
        }

        links.push(Link {
            id: format!("link-{link_id}"),
            source_id,
            target_id,
        });
    }

    for stream in &mut streams {
        stream.current_target = stream_targets.get(&stream.id).cloned();
    }

    streams.sort_by(|a, b| a.app_name.cmp(&b.app_name));

    let mut graph = RuntimeGraph {
        devices,
        streams,
        links,
        data_source: "pipewire".into(),
        notice: None,
        ..Default::default()
    };

    graph_enrich::finalize_graph(&mut graph);
    graph
}

pub(crate) fn device_label(props: &serde_json::Map<String, Value>, media_class: &str) -> String {
    let profile = prop_str(props, "device.profile.description");
    let card = prop_str(props, "api.alsa.card.name");
    let description = prop_str(props, "node.description");
    let node_name = prop_str(props, "node.name");

    if media_class == "Audio/Sink" {
        if profile.is_empty() && description.is_empty() && card.is_empty() && !node_name.is_empty() {
            return prettify_node_name(&node_name);
        }

        let kind = if profile.contains("HDMI") || description.contains("HDMI") {
            "HDMI / DP"
        } else if profile.contains("Analog") || description.contains("Analog") {
            "Analog Output"
        } else if !profile.is_empty() {
            profile.as_str()
        } else {
            return fallback_device_label(&description, &card, &node_name);
        };

        return format!("{} - {}", kind, device_short_name(&card, &description));
    }

    if is_source_media_class(media_class) {
        if is_virtual_device(props) {
            return fallback_device_label(&description, &card, &node_name);
        }

        let label = format!(
            "Microphone - {}",
            device_short_name(&card, &description)
        );
        if label != "Microphone - " {
            return label;
        }
    }

    fallback_device_label(&description, &card, &node_name)
}

fn object_props(info: &Option<Value>) -> Option<&serde_json::Map<String, Value>> {
    info.as_ref()?.get("props")?.as_object()
}

fn prop_str(props: &serde_json::Map<String, Value>, key: &str) -> String {
    props
        .get(key)
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

fn value_as_u32(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().map(|n| n as u32),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn node_id(id: u32) -> String {
    format!("node-{id}")
}

fn should_skip_media_class(media_class: &str) -> bool {
    media_class.is_empty()
        || media_class.starts_with("Midi/")
        || media_class.starts_with("Video/")
}

fn is_source_media_class(media_class: &str) -> bool {
    media_class.starts_with("Audio/Source")
}

fn is_hardware_audio_endpoint(props: &serde_json::Map<String, Value>) -> bool {
    if props.contains_key("api.alsa.pcm.card") || props.contains_key("api.alsa.pcm.device") {
        return true;
    }

    matches!(
        prop_str(props, "device.api").as_str(),
        "alsa" | "bluez5" | "v4l2"
    )
}

fn is_system_stream(props: &serde_json::Map<String, Value>) -> bool {
    let app_name = prop_str(props, "application.name");
    let process = prop_str(props, "application.process.binary");
    let node_name = prop_str(props, "node.name");

    is_internal_audio_client(&app_name)
        || is_internal_audio_client(&process)
        || is_internal_audio_client(&node_name)
}

fn is_virtual_device(props: &serde_json::Map<String, Value>) -> bool {
    let factory = prop_str(props, "factory.name");
    if factory.contains("support.null") || factory.contains("adapter") {
        return true;
    }

    if props.contains_key("api.alsa.pcm.card") || props.contains_key("api.alsa.pcm.device") {
        return false;
    }

    let device_api = prop_str(props, "device.api");
    if device_api == "alsa" || device_api == "v4l2" || device_api == "bluez5" {
        return false;
    }

    let node_name = prop_str(props, "node.name");
    node_name.starts_with("pipe-deck-")
        || node_name.contains("null")
        || node_name.contains("easyeffects")
}

fn device_short_name(card: &str, description: &str) -> String {
    if description.contains("GA102") {
        return "GA102 HD Audio".into();
    }

    if !card.is_empty() {
        return card.to_string();
    }

    description
        .split(" Analog")
        .next()
        .unwrap_or(description)
        .split(" Mono")
        .next()
        .unwrap_or(description)
        .split(" Digital")
        .next()
        .unwrap_or(description)
        .trim()
        .to_string()
}

fn fallback_device_label(description: &str, card: &str, node_name: &str) -> String {
    if !description.is_empty() {
        return description.to_string();
    }
    if !card.is_empty() {
        return card.to_string();
    }
    if !node_name.is_empty() {
        return prettify_node_name(node_name);
    }
    "Unknown Device".into()
}

fn prettify_node_name(node_name: &str) -> String {
    node_name
        .replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn stream_media_name(props: &serde_json::Map<String, Value>) -> Option<String> {
    let media_name = prop_str(props, "media.name");
    if media_name.is_empty() {
        return None;
    }
    Some(media_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdmi_sink_ignores_monitor_nick() {
        let props = serde_json::json!({
            "media.class": "Audio/Sink",
            "node.nick": "LG ULTRAWIDE",
            "node.description": "GA102 High Definition Audio Controller Digital Stereo (HDMI)",
            "device.profile.description": "Digital Stereo (HDMI)",
            "api.alsa.card.name": "HDA NVidia",
            "node.name": "alsa_output.pci-0000_01_00.1.hdmi-stereo",
        });

        let label = device_label(
            props.as_object().expect("props object"),
            "Audio/Sink",
        );
        assert_eq!(label, "HDMI / DP - GA102 HD Audio");
    }

    #[test]
    fn analog_sink_uses_profile_and_card() {
        let props = serde_json::json!({
            "media.class": "Audio/Sink",
            "node.description": "Arctis Nova Pro Wireless Analog Stereo",
            "device.profile.description": "Analog Stereo",
            "api.alsa.card.name": "Arctis Nova Pro Wireless",
            "node.name": "alsa_output.usb-headset.analog-stereo",
        });

        let label = device_label(
            props.as_object().expect("props object"),
            "Audio/Sink",
        );
        assert_eq!(label, "Analog Output - Arctis Nova Pro Wireless");
    }

    #[test]
    fn source_uses_microphone_prefix() {
        let props = serde_json::json!({
            "media.class": "Audio/Source",
            "node.description": "Arctis Nova Pro Wireless Mono",
            "api.alsa.card.name": "Arctis Nova Pro Wireless",
            "node.name": "alsa_input.usb-headset.mono-fallback",
        });

        let label = device_label(
            props.as_object().expect("props object"),
            "Audio/Source",
        );
        assert_eq!(label, "Microphone - Arctis Nova Pro Wireless");
    }

    #[test]
    fn virtual_source_media_class_is_discovered() {
        let objects = vec![PwDumpObject {
            id: 99,
            object_type: "PipeWire:Interface:Node".into(),
            info: Some(serde_json::json!({
                "props": {
                    "media.class": "Audio/Source/Virtual",
                    "node.name": "soundux-custom-mic",
                    "node.description": "Custom Mic",
                    "factory.name": "support.null-audio-sink"
                }
            })),
        }];

        let graph = normalize(&objects);
        assert_eq!(graph.devices.len(), 1);
        assert_eq!(graph.devices[0].system_name, "soundux-custom-mic");
        assert_eq!(graph.devices[0].direction, DeviceDirection::Input);
        assert_eq!(graph.devices[0].kind, DeviceKind::Virtual);
    }

    #[test]
    fn pipe_deck_devices_are_left_to_virtual_registry() {
        let objects = vec![PwDumpObject {
            id: 99,
            object_type: "PipeWire:Interface:Node".into(),
            info: Some(serde_json::json!({
                "props": {
                    "media.class": "Audio/Source/Virtual",
                    "node.name": "pipe-deck-test-mic",
                    "node.description": "Test Mic",
                    "factory.name": "support.null-audio-sink"
                }
            })),
        }];

        let graph = normalize(&objects);
        assert!(graph.devices.is_empty());
    }

    #[test]
    fn skips_fabricated_media_classes() {
        let objects = vec![
            PwDumpObject {
                id: 35,
                object_type: "PipeWire:Interface:Node".into(),
                info: Some(serde_json::json!({
                    "props": {
                        "media.class": "Audio/Sink",
                        "node.name": "alsa_output.usb-headset",
                        "node.description": "Headset Analog Stereo",
                        "device.profile.description": "Analog Stereo",
                        "api.alsa.card.name": "Headset",
                        "api.alsa.pcm.card": "1"
                    }
                })),
            },
            PwDumpObject {
                id: 75,
                object_type: "PipeWire:Interface:Node".into(),
                info: Some(serde_json::json!({
                    "props": {
                        "media.class": "Stream/Output/Audio",
                        "application.name": "Firefox"
                    }
                })),
            },
            PwDumpObject {
                id: 78,
                object_type: "PipeWire:Interface:Link".into(),
                info: Some(serde_json::json!({
                    "output-node-id": 75,
                    "input-node-id": 35
                })),
            },
        ];

        let graph = normalize(&objects);
        assert_eq!(graph.data_source, "pipewire");
        assert_eq!(graph.devices.len(), 1);
        assert_eq!(graph.streams.len(), 1);
        assert_eq!(graph.streams[0].app_name, "Firefox");
        assert_eq!(graph.streams[0].current_target.as_deref(), Some("node-35"));
    }
}
