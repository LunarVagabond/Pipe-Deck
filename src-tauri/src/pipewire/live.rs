use crate::config::ConfigStore;
use crate::core::models::{
    Device, DeviceDirection, DeviceKind, Link, RuntimeGraph, Stream, StreamDirection,
};
use crate::pipewire::adapter::{AdapterError, GraphListener, PipeWireAdapter};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub struct LivePipeWireAdapter {
    cached_graph: Arc<Mutex<RuntimeGraph>>,
    listener: Arc<Mutex<Option<GraphListener>>>,
}

impl LivePipeWireAdapter {
    pub fn new() -> Result<Self, AdapterError> {
        let graph = enumerate_pipewire()?;
        let cached_graph = Arc::new(Mutex::new(graph));
        let listener = Arc::new(Mutex::new(None));

        Ok(Self {
            cached_graph,
            listener,
        })
    }
}

impl PipeWireAdapter for LivePipeWireAdapter {
    fn fetch_graph(&self) -> Result<RuntimeGraph, AdapterError> {
        let graph = enumerate_pipewire()?;
        let mut cached = self.cached_graph.lock().map_err(|_| {
            AdapterError::Message("graph lock poisoned".into())
        })?;
        *cached = graph.clone();
        Ok(graph)
    }

    fn subscribe(&self, listener: GraphListener) -> Result<(), AdapterError> {
        *self.listener.lock().map_err(|_| {
            AdapterError::Message("listener lock poisoned".into())
        })? = Some(listener);

        // Poller is started once via leak-safe approach: store in adapter
        // Note: subscribe is called once from engine; poller thread needs to be spawned.
        // We spawn here but can't store JoinHandle without &mut self.
        // Use a static or spawn detached - detached is fine for app lifetime.
        let cached_graph = self.cached_graph.clone();
        let listener_slot = self.listener.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(POLL_INTERVAL);
                let Ok(next_graph) = enumerate_pipewire() else {
                    continue;
                };
                let changed = {
                    let mut current = cached_graph.lock().expect("graph lock poisoned");
                    if *current != next_graph {
                        *current = next_graph.clone();
                        true
                    } else {
                        false
                    }
                };
                if changed {
                    if let Some(callback) =
                        listener_slot.lock().expect("listener lock poisoned").as_ref()
                    {
                        callback(next_graph);
                    }
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct PwDumpObject {
    id: u32,
    #[serde(rename = "type")]
    object_type: String,
    info: Option<Value>,
}

fn enumerate_pipewire() -> Result<RuntimeGraph, AdapterError> {
    let output = Command::new("pw-dump")
        .arg("-N")
        .output()
        .map_err(|error| AdapterError::Message(format!("failed to run pw-dump: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AdapterError::Message(format!(
            "pw-dump failed: {stderr}"
        )));
    }

    let objects: Vec<PwDumpObject> = serde_json::from_slice(&output.stdout).map_err(|error| {
        AdapterError::Message(format!("failed to parse pw-dump output: {error}"))
    })?;

    Ok(normalize_pw_dump(&objects))
}

fn normalize_pw_dump(objects: &[PwDumpObject]) -> RuntimeGraph {
    let mut devices = Vec::new();
    let mut streams = Vec::new();
    let mut raw_links: Vec<(u32, u32, u32)> = Vec::new();

    for object in objects {
        if object.object_type.ends_with("Interface:Node") {
            let Some(props) = object_props(&object.info) else {
                continue;
            };

            let media_class = prop_str(&props, "media.class");
            if should_skip_media_class(&media_class) {
                continue;
            }

            let node_name = prop_str(&props, "node.name");
            if node_name.contains(".monitor") {
                continue;
            }

            let id = node_id(object.id);

            if media_class.contains("Stream/Output") {
                streams.push(Stream {
                    id: id.clone(),
                    app_name: stream_label(&props),
                    direction: StreamDirection::Playback,
                    current_target: None,
                    is_system: is_system_stream(&props),
                });
                continue;
            }

            if media_class.contains("Stream/Input") {
                streams.push(Stream {
                    id: id.clone(),
                    app_name: stream_label(&props),
                    direction: StreamDirection::Capture,
                    current_target: None,
                    is_system: is_system_stream(&props),
                });
                continue;
            }

            if media_class == "Audio/Sink" {
                devices.push(Device {
                    id: id.clone(),
                    system_name: node_name.clone(),
                    label: device_label(&props, &media_class),
                    kind: if is_virtual_device(&props) {
                        DeviceKind::Virtual
                    } else {
                        DeviceKind::Physical
                    },
                    direction: DeviceDirection::Output,
                    volume_percent: None,
                    muted: None,
                });
                continue;
            }

            if media_class == "Audio/Source" {
                devices.push(Device {
                    id,
                    system_name: node_name,
                    label: device_label(&props, &media_class),
                    kind: if is_virtual_device(&props) {
                        DeviceKind::Virtual
                    } else {
                        DeviceKind::Physical
                    },
                    direction: DeviceDirection::Input,
                    volume_percent: None,
                    muted: None,
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
    };

    finalize_graph(&mut graph);
    graph
}

fn finalize_graph(graph: &mut RuntimeGraph) {
    apply_device_aliases(&mut graph.devices);
    apply_pactl_levels(&mut graph.devices);
}

fn apply_device_aliases(devices: &mut [Device]) {
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

fn is_system_stream(props: &serde_json::Map<String, Value>) -> bool {
    let app_name = prop_str(props, "application.name");
    let process = prop_str(props, "application.process.binary");
    let node_name = prop_str(props, "node.name");

    // Accessibility subsystem placeholder stream, not a user-facing app.
    app_name.contains("speech-dispatcher")
        || process.contains("speech-dispatcher")
        || node_name.contains("speech-dispatcher")
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
    node_name.contains("null") || node_name.contains("easyeffects")
}

fn device_label(props: &serde_json::Map<String, Value>, media_class: &str) -> String {
    let profile = prop_str(props, "device.profile.description");
    let card = prop_str(props, "api.alsa.card.name");
    let description = prop_str(props, "node.description");

    if media_class == "Audio/Sink" {
        let kind = if profile.contains("HDMI") || description.contains("HDMI") {
            "HDMI / DP"
        } else if profile.contains("Analog") || description.contains("Analog") {
            "Analog Output"
        } else if !profile.is_empty() {
            profile.as_str()
        } else {
            return fallback_device_label(&description, &card);
        };

        return format!("{} - {}", kind, device_short_name(&card, &description));
    }

    if media_class == "Audio/Source" {
        return format!(
            "Microphone - {}",
            device_short_name(&card, &description)
        );
    }

    fallback_device_label(&description, &card)
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

fn fallback_device_label(description: &str, card: &str) -> String {
    if !description.is_empty() {
        return description.to_string();
    }
    if !card.is_empty() {
        return card.to_string();
    }
    "Unknown Device".into()
}

fn stream_label(props: &serde_json::Map<String, Value>) -> String {
    for key in [
        "application.name",
        "application.process.binary",
        "media.name",
        "node.nick",
        "node.name",
    ] {
        let value = prop_str(props, key);
        if !value.is_empty() {
            return value;
        }
    }
    "Unknown Stream".into()
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

        let graph = normalize_pw_dump(&objects);
        assert_eq!(graph.data_source, "pipewire");
        assert_eq!(graph.devices.len(), 1);
        assert_eq!(graph.streams.len(), 1);
        assert_eq!(graph.streams[0].app_name, "Firefox");
        assert_eq!(graph.streams[0].current_target.as_deref(), Some("node-35"));
    }
}
