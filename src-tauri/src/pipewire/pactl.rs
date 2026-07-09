use crate::core::models::{DeviceDirection, RuntimeGraph, StreamDirection};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pw_link;
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PactlSinkInput {
    pub index: u32,
    pub application_name: String,
    pub node_name: Option<String>,
    pub media_name: Option<String>,
    pub sink_index: Option<u32>,
}

pub fn list_sink_inputs() -> Vec<PactlSinkInput> {
    parse_sink_inputs()
}

pub fn load_sink_index_names() -> HashMap<u32, String> {
    let output = match Command::new("pactl").args(["list", "sinks", "short"]).output() {
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

pub fn move_stream_to_target(
    graph: &RuntimeGraph,
    stream_id: &str,
    target_device_id: &str,
) -> Result<(), AdapterError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| AdapterError::Message(format!("stream not found: {stream_id}")))?;

    let target = graph
        .devices
        .iter()
        .find(|device| device.id == target_device_id)
        .ok_or_else(|| AdapterError::Message(format!("target device not found: {target_device_id}")))?;

    match stream.direction {
        StreamDirection::Playback => {
            let sink_name = resolve_playback_sink_name(target)?;
            if !matches!(target.direction, DeviceDirection::Output | DeviceDirection::Duplex | DeviceDirection::Input) {
                return Err(AdapterError::Message(
                    "playback streams must target an output or virtual input".into(),
                ));
            }
            let input_index = find_sink_input_index(graph, stream)?;
            run_pactl(&["move-sink-input", &input_index.to_string(), &sink_name])?;
        }
        StreamDirection::Capture => {
            if !matches!(target.direction, DeviceDirection::Input | DeviceDirection::Duplex) {
                return Err(AdapterError::Message(
                    "capture streams must target an input device".into(),
                ));
            }
            let output_index = find_source_output_index(graph, stream)?;
            run_pactl(&[
                "move-source-output",
                &output_index.to_string(),
                &target.system_name,
            ])?;
        }
    }

    Ok(())
}

fn resolve_playback_sink_name(target: &crate::core::models::Device) -> Result<String, AdapterError> {
    if target.direction == DeviceDirection::Input && target.kind == crate::core::models::DeviceKind::Virtual {
        let feed_sink = ensure_feed_sink_for_virtual_input(&target.system_name, &target.label)?;
        pw_link::link_sink_monitor_to_target(&feed_sink, &target.system_name, true)?;
        return Ok(feed_sink);
    }

    if !matches!(target.direction, DeviceDirection::Output | DeviceDirection::Duplex) {
        return Err(AdapterError::Message(
            "playback streams must target an output device".into(),
        ));
    }

    Ok(target.system_name.clone())
}

fn ensure_feed_sink_for_virtual_input(
    virtual_input_system_name: &str,
    label: &str,
) -> Result<String, AdapterError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    let description = feed_sink_description(label);

    if sink_exists(&feed_name)? {
        sync_feed_sink_description(&feed_name, virtual_input_system_name, &description)?;
        return Ok(feed_name);
    }

    create_null_sink(&feed_name, &description)?;
    Ok(feed_name)
}

pub fn feed_sink_description(virtual_mic_label: &str) -> String {
    format!("{virtual_mic_label} (Pipe Deck route)")
}

pub fn sync_feed_sink_for_virtual_input(
    virtual_input_system_name: &str,
    label: &str,
) -> Result<(), AdapterError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    if !sink_exists(&feed_name)? {
        return Ok(());
    }

    sync_feed_sink_description(
        &feed_name,
        virtual_input_system_name,
        &feed_sink_description(label),
    )
}

fn sync_feed_sink_description(
    feed_name: &str,
    virtual_input_system_name: &str,
    description: &str,
) -> Result<(), AdapterError> {
    if sink_description(feed_name)?.as_deref() == Some(description) {
        return Ok(());
    }

    if feed_sink_in_use(feed_name)? {
        return Ok(());
    }

    remove_feed_sink_for_virtual_input(virtual_input_system_name)?;
    create_null_sink(feed_name, description)?;
    Ok(())
}

fn feed_sink_in_use(feed_name: &str) -> Result<bool, AdapterError> {
    let sink_names = load_sink_index_names();
    Ok(list_sink_inputs().iter().any(|input| {
        input
            .sink_index
            .and_then(|index| sink_names.get(&index))
            .is_some_and(|name| name == feed_name)
    }))
}

fn sink_description(name: &str) -> Result<Option<String>, AdapterError> {
    let output = run_pactl(&["list", "sinks"])?;
    let mut current_name = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Name: ") {
            current_name = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Description: ") {
            if current_name.as_deref() == Some(name) {
                return Ok(Some(rest.trim().to_string()));
            }
        }
    }

    Ok(None)
}

pub fn feed_sink_name_for_virtual_input(virtual_input_system_name: &str) -> String {
    let slug = virtual_input_system_name
        .strip_prefix("pipe-deck-")
        .unwrap_or(virtual_input_system_name);
    format!("pipe-deck-feed-{slug}")
}

pub fn remove_feed_sink_for_virtual_input(virtual_input_system_name: &str) -> Result<(), AdapterError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    let _ = pw_link::disconnect_sink_monitor(&feed_name);
    if let Some(module_id) = find_module_id_by_sink_name(&feed_name)? {
        unload_module(&module_id)?;
    }
    Ok(())
}

pub fn gc_feed_sinks(known_virtual_inputs: &std::collections::HashSet<String>) -> Result<(), AdapterError> {
    let sink_names = load_sink_index_names();
    let sinks_with_inputs: std::collections::HashSet<String> = list_sink_inputs()
        .iter()
        .filter_map(|input| {
            input
                .sink_index
                .and_then(|index| sink_names.get(&index).cloned())
        })
        .collect();

    for (module_id, feed_name) in list_modules_for_sink_prefix("pipe-deck-feed-")? {
        let Some(slug) = feed_name.strip_prefix("pipe-deck-feed-") else {
            continue;
        };
        let virtual_input = format!("pipe-deck-{slug}");
        let virtual_exists = known_virtual_inputs.contains(&virtual_input);
        let in_use = sinks_with_inputs.contains(&feed_name);

        if virtual_exists && in_use {
            continue;
        }

        let _ = pw_link::disconnect_sink_monitor(&feed_name);
        unload_module(&module_id)?;
    }

    Ok(())
}

fn sink_exists(name: &str) -> Result<bool, AdapterError> {
    let output = run_pactl(&["list", "sinks", "short"])?;
    Ok(output.lines().any(|line| line.split_whitespace().nth(1) == Some(name)))
}

pub fn set_device_volume(device_id: &str, graph: &RuntimeGraph, percent: u8) -> Result<(), AdapterError> {
    let device = graph
        .devices
        .iter()
        .find(|device| device.id == device_id)
        .ok_or_else(|| AdapterError::Message(format!("device not found: {device_id}")))?;

    let percent = percent.min(100);
    match device.direction {
        DeviceDirection::Output | DeviceDirection::Duplex => {
            run_pactl(&[
                "set-sink-volume",
                &device.system_name,
                &format!("{percent}%"),
            ])?;
        }
        DeviceDirection::Input => {
            run_pactl(&[
                "set-source-volume",
                &device.system_name,
                &format!("{percent}%"),
            ])?;
        }
    }
    Ok(())
}

pub fn set_device_mute(device_id: &str, graph: &RuntimeGraph, muted: bool) -> Result<(), AdapterError> {
    let device = graph
        .devices
        .iter()
        .find(|device| device.id == device_id)
        .ok_or_else(|| AdapterError::Message(format!("device not found: {device_id}")))?;

    let flag = if muted { "1" } else { "0" };
    match device.direction {
        DeviceDirection::Output | DeviceDirection::Duplex => {
            run_pactl(&["set-sink-mute", &device.system_name, flag])?;
        }
        DeviceDirection::Input => {
            run_pactl(&["set-source-mute", &device.system_name, flag])?;
        }
    }
    Ok(())
}

pub fn create_null_sink(name: &str, description: &str) -> Result<String, AdapterError> {
    let description = escape_sink_property(description);
    let output = run_pactl(&[
        "load-module",
        "module-null-sink",
        &format!("sink_name={name}"),
        &format!(
            "sink_properties=device.description=\"{description}\" node.description=\"{description}\" node.nick=\"{description}\""
        ),
    ])?;
    Ok(output.trim().to_string())
}

fn escape_sink_property(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// PipeWire does not provide `module-null-source`. Create a virtual capture
/// endpoint using a null sink configured as an Audio/Source node.
pub fn create_virtual_source(name: &str, description: &str) -> Result<String, AdapterError> {
    let output = run_pactl(&[
        "load-module",
        "module-null-sink",
        "media.class=Audio/Source/Virtual",
        &format!("sink_name={name}"),
        &format!("sink_properties=device.description=\"{description}\""),
        "channel_map=front-left,front-right",
    ])?;
    Ok(output.trim().to_string())
}

pub fn find_module_id_by_sink_name(sink_name: &str) -> Result<Option<String>, AdapterError> {
    let output = run_pactl(&["list", "modules", "short"])?;
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(index) = parts.next() else {
            continue;
        };
        let args = parts.collect::<Vec<_>>().join(" ");
        if args.contains(&format!("sink_name={sink_name}")) {
            return Ok(Some(index.to_string()));
        }
    }
    Ok(None)
}

pub fn list_pipe_deck_modules() -> Result<Vec<PactlVirtualModule>, AdapterError> {
    let output = run_pactl(&["list", "modules", "short"])?;
    let mut entries = Vec::new();

    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(module_id) = parts.next() else {
            continue;
        };
        let args = parts.collect::<Vec<_>>().join(" ");
        let Some(system_name) = extract_arg_value(&args, "sink_name=") else {
            continue;
        };
        if !system_name.starts_with("pipe-deck-") || system_name.starts_with("pipe-deck-feed-") {
            continue;
        }

        let slug = system_name.strip_prefix("pipe-deck-").unwrap_or(&system_name);
        let direction = if args.contains("media.class=Audio/Source/Virtual") {
            DeviceDirection::Input
        } else {
            DeviceDirection::Output
        };
        let label = extract_description(&args).unwrap_or_else(|| system_name.clone());

        entries.push(PactlVirtualModule {
            module_id: module_id.to_string(),
            device_id: format!("virtual-{slug}"),
            system_name,
            label,
            direction,
        });
    }

    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct PactlVirtualModule {
    pub module_id: String,
    pub device_id: String,
    pub system_name: String,
    pub label: String,
    pub direction: DeviceDirection,
}

fn list_modules_for_sink_prefix(prefix: &str) -> Result<Vec<(String, String)>, AdapterError> {
    let output = run_pactl(&["list", "modules", "short"])?;
    let mut entries = Vec::new();

    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(module_id) = parts.next() else {
            continue;
        };
        let args = parts.collect::<Vec<_>>().join(" ");
        let Some(sink_name) = extract_arg_value(&args, "sink_name=") else {
            continue;
        };
        if sink_name.starts_with(prefix) {
            entries.push((module_id.to_string(), sink_name));
        }
    }

    Ok(entries)
}

fn extract_arg_value(args: &str, prefix: &str) -> Option<String> {
    let start = args.find(prefix)? + prefix.len();
    let rest = &args[start..];
    if rest.starts_with('"') {
        let end = rest[1..].find('"')? + 1;
        return Some(rest[1..end].to_string());
    }
    let end = rest.find(' ').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn extract_description(args: &str) -> Option<String> {
    let marker = "device.description=\"";
    let start = args.find(marker)? + marker.len();
    let rest = &args[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub fn unload_module(module_id: &str) -> Result<(), AdapterError> {
    run_pactl(&["unload-module", module_id]).map(|_| ())
}

fn run_pactl(args: &[&str]) -> Result<String, AdapterError> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| AdapterError::Message(format!("failed to run pactl: {error}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AdapterError::Message(format!(
        "pactl {} failed: {stderr}",
        args.join(" ")
    )))
}

fn find_sink_input_index(graph: &RuntimeGraph, stream: &crate::core::models::Stream) -> Result<u32, AdapterError> {
    let entries = parse_pactl_clients("sink-inputs");
    find_client_index(&entries, graph, stream)
}

fn find_source_output_index(
    graph: &RuntimeGraph,
    stream: &crate::core::models::Stream,
) -> Result<u32, AdapterError> {
    let entries = parse_pactl_clients("source-outputs");
    find_client_index(&entries, graph, stream)
}

struct PactlClientEntry {
    index: u32,
    application_name: Option<String>,
    node_name: Option<String>,
}

fn parse_pactl_clients(kind: &str) -> Vec<PactlClientEntry> {
    let output = match Command::new("pactl").args(["list", kind]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    let mut current_index = None;
    let mut current_app = None;
    let mut current_node = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Sink Input #") {
            if let Some(index) = current_index.take() {
                entries.push(PactlClientEntry {
                    index,
                    application_name: current_app.take(),
                    node_name: current_node.take(),
                });
            }
            current_index = rest.parse().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix("Source Output #") {
            if let Some(index) = current_index.take() {
                entries.push(PactlClientEntry {
                    index,
                    application_name: current_app.take(),
                    node_name: current_node.take(),
                });
            }
            current_index = rest.parse().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix("application.name = ") {
            current_app = Some(rest.trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("node.name = ") {
            current_node = Some(rest.trim_matches('"').to_string());
        }
    }

    if let Some(index) = current_index {
        entries.push(PactlClientEntry {
            index,
            application_name: current_app,
            node_name: current_node,
        });
    }

    entries
}

fn find_client_index(
    entries: &[PactlClientEntry],
    _graph: &RuntimeGraph,
    stream: &crate::core::models::Stream,
) -> Result<u32, AdapterError> {
    if let Some(rest) = stream.id.strip_prefix("pactl-sink-input-") {
        if let Ok(index) = rest.parse::<u32>() {
            if entries.iter().any(|entry| entry.index == index) {
                return Ok(index);
            }
        }
    }

    if let Some(system_name) = &stream.system_name {
        if let Some(entry) = entries.iter().find(|entry| entry.node_name.as_deref() == Some(system_name.as_str())) {
            return Ok(entry.index);
        }
    }

    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.application_name.as_deref() == Some(stream.app_name.as_str()))
    {
        return Ok(entry.index);
    }

    Err(AdapterError::Message(format!(
        "could not find pactl client for stream {}",
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
    let mut current_node = None;
    let mut current_media = None;
    let mut current_sink = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Sink Input #") {
            if let Some(index) = current_index.take() {
                if let Some(application_name) = current_app.take() {
                    inputs.push(PactlSinkInput {
                        index,
                        application_name,
                        node_name: current_node.take(),
                        media_name: current_media.take(),
                        sink_index: current_sink.take(),
                    });
                }
            }
            current_index = rest.parse().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix("application.name = ") {
            current_app = Some(rest.trim_matches('"').to_string());
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
        }
    }

    if let Some(index) = current_index {
        if let Some(application_name) = current_app {
            inputs.push(PactlSinkInput {
                index,
                application_name,
                node_name: current_node,
                media_name: current_media,
                sink_index: current_sink,
            });
        }
    }

    inputs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_sink_name_derives_from_virtual_input() {
        assert_eq!(
            feed_sink_name_for_virtual_input("pipe-deck-test"),
            "pipe-deck-feed-test"
        );
        assert_eq!(
            feed_sink_name_for_virtual_input("pipe-deck-virtual-input"),
            "pipe-deck-feed-virtual-input"
        );
    }

    #[test]
    fn feed_sink_description_uses_virtual_mic_label() {
        assert_eq!(
            feed_sink_description("YouTube to Discord"),
            "YouTube to Discord (Pipe Deck route)"
        );
    }
}
