use crate::core::models::{DeviceDirection, DeviceKind, RuntimeGraph, StreamDirection};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl::parse::{find_sink_input_index, find_source_output_index};
use crate::pipewire::pactl::run_pactl;
use crate::pipewire::pactl::r#virtual::{
    create_null_sink, create_virtual_source, ensure_feed_sink_for_virtual_input,
    feed_sink_name_for_virtual_input, sink_exists,
};
use crate::pipewire::pw_link;
use std::collections::HashSet;
use std::process::Command;

pub fn move_stream_to_target(
    graph: &RuntimeGraph,
    stream_id: &str,
    target_device_id: &str,
) -> Result<(), AdapterError> {
    let target = graph
        .devices
        .iter()
        .find(|device| device.id == target_device_id)
        .ok_or_else(|| AdapterError::Message(format!("target device not found: {target_device_id}")))?;

    move_stream_to_resolved_target(graph, stream_id, target)
}

pub fn move_stream_to_sink_name(
    graph: &RuntimeGraph,
    stream_id: &str,
    sink_system_name: &str,
) -> Result<(), AdapterError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| AdapterError::Message(format!("stream not found: {stream_id}")))?;

    if stream.direction != StreamDirection::Playback {
        return Err(AdapterError::Message(
            "only playback streams can be moved to a sink".into(),
        ));
    }

    let input_index = find_sink_input_index(graph, stream)?;
    run_pactl(&[
        "move-sink-input",
        &input_index.to_string(),
        sink_system_name,
    ])?;
    Ok(())
}

const UNROUTED_PLAYBACK_SINK: &str = "pipe-deck-unrouted";
const UNROUTED_CAPTURE_SOURCE: &str = "pipe-deck-unrouted-capture";

pub fn clear_stream_target(
    graph: &RuntimeGraph,
    stream_id: &str,
    avoid_target_device_id: Option<&str>,
) -> Result<(), AdapterError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| AdapterError::Message(format!("stream not found: {stream_id}")))?;

    match stream.direction {
        StreamDirection::Playback => {
            let index = match find_sink_input_index(graph, stream) {
                Ok(index) => index,
                Err(_) => return Ok(()),
            };
            let avoid = avoid_sink_system_names(graph, avoid_target_device_id);
            let fallback = resolve_clear_playback_sink(graph, &avoid)?;
            move_sink_input_with_fallback(index, &fallback)?;
        }
        StreamDirection::Capture => {
            let index = match find_source_output_index(graph, stream) {
                Ok(index) => index,
                Err(_) => return Ok(()),
            };
            let avoid = avoid_source_system_names(graph, avoid_target_device_id);
            let fallback = resolve_clear_capture_source(graph, &avoid)?;
            move_source_output_with_fallback(index, &fallback)?;
        }
    }

    Ok(())
}

fn move_sink_input_with_fallback(index: u32, sink_name: &str) -> Result<(), AdapterError> {
    if run_pactl(&["move-sink-input", &index.to_string(), sink_name]).is_ok() {
        return Ok(());
    }
    ensure_unrouted_playback_sink()?;
    run_pactl(&[
        "move-sink-input",
        &index.to_string(),
        UNROUTED_PLAYBACK_SINK,
    ])?;
    Ok(())
}

fn move_source_output_with_fallback(index: u32, source_name: &str) -> Result<(), AdapterError> {
    if run_pactl(&["move-source-output", &index.to_string(), source_name]).is_ok() {
        return Ok(());
    }
    ensure_unrouted_capture_source()?;
    run_pactl(&[
        "move-source-output",
        &index.to_string(),
        UNROUTED_CAPTURE_SOURCE,
    ])?;
    Ok(())
}

fn ensure_unrouted_playback_sink() -> Result<(), AdapterError> {
    if sink_exists(UNROUTED_PLAYBACK_SINK)? {
        return Ok(());
    }
    create_null_sink(UNROUTED_PLAYBACK_SINK, "Unrouted")?;
    Ok(())
}

fn ensure_unrouted_capture_source() -> Result<(), AdapterError> {
    if sink_exists(UNROUTED_CAPTURE_SOURCE)? {
        return Ok(());
    }
    create_virtual_source(UNROUTED_CAPTURE_SOURCE, "Unrouted Capture")?;
    Ok(())
}

fn avoid_sink_system_names(graph: &RuntimeGraph, avoid_device_id: Option<&str>) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(device_id) = avoid_device_id else {
        return names;
    };
    let Some(device) = graph.devices.iter().find(|device| device.id == device_id) else {
        return names;
    };
    names.insert(device.system_name.clone());
    if device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Input {
        names.insert(feed_sink_name_for_virtual_input(&device.system_name));
    }
    names
}

fn avoid_source_system_names(
    graph: &RuntimeGraph,
    avoid_device_id: Option<&str>,
) -> HashSet<String> {
    avoid_sink_system_names(graph, avoid_device_id)
}

fn resolve_clear_playback_sink(
    graph: &RuntimeGraph,
    avoid: &HashSet<String>,
) -> Result<String, AdapterError> {
    if let Some(default_sink) = get_default_sink_name() {
        if !avoid.contains(&default_sink) {
            return Ok(default_sink);
        }
    }

    for device in &graph.devices {
        if device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Input {
            continue;
        }
        if !matches!(
            device.direction,
            DeviceDirection::Output | DeviceDirection::Duplex
        ) {
            continue;
        }
        if avoid.contains(&device.system_name) {
            continue;
        }
        return Ok(device.system_name.clone());
    }

    ensure_unrouted_playback_sink()?;
    Ok(UNROUTED_PLAYBACK_SINK.to_string())
}

fn resolve_clear_capture_source(
    graph: &RuntimeGraph,
    avoid: &HashSet<String>,
) -> Result<String, AdapterError> {
    if let Some(default_source) = get_default_source_name() {
        if !avoid.contains(&default_source) {
            return Ok(default_source);
        }
    }

    for device in &graph.devices {
        if !matches!(
            device.direction,
            DeviceDirection::Input | DeviceDirection::Duplex
        ) {
            continue;
        }
        if avoid.contains(&device.system_name) {
            continue;
        }
        return Ok(device.system_name.clone());
    }

    ensure_unrouted_capture_source()?;
    Ok(UNROUTED_CAPTURE_SOURCE.to_string())
}

fn get_default_sink_name() -> Option<String> {
    read_pactl_default_name(&["get-default-sink"])
}

fn get_default_source_name() -> Option<String> {
    read_pactl_default_name(&["get-default-source"])
}

fn read_pactl_default_name(args: &[&str]) -> Option<String> {
    let output = Command::new("pactl").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn move_stream_to_resolved_target(
    graph: &RuntimeGraph,
    stream_id: &str,
    target: &crate::core::models::Device,
) -> Result<(), AdapterError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| AdapterError::Message(format!("stream not found: {stream_id}")))?;

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
