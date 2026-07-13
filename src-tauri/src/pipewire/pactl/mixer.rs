use crate::core::models::{DeviceDirection, DeviceKind, RuntimeGraph, StreamDirection};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl::parse::{find_sink_input_index, find_source_output_index};
use crate::pipewire::pactl::run_pactl;

/// Moves a single sink-input (an app's playback stream) onto a different
/// sink by raw name, bypassing `RuntimeGraph` lookup. Used to temporarily
/// hold an in-use device's streams elsewhere while its underlying module is
/// swapped out for an effects-hosted one, then move them back — see
/// `core::engine::effects_ops::apply_effect_chain_structural`.
pub fn move_sink_input_to_sink_name(sink_input_index: u32, target_sink_name: &str) -> Result<(), AdapterError> {
    run_pactl(&["move-sink-input", &sink_input_index.to_string(), target_sink_name]).map(|_| ())
}

pub fn set_device_volume(device_id: &str, graph: &RuntimeGraph, percent: u8) -> Result<(), AdapterError> {
    let device = graph
        .devices
        .iter()
        .find(|device| device.id == device_id)
        .ok_or_else(|| AdapterError::Message(format!("device not found: {device_id}")))?;

    let percent = percent.min(100);
    let volume_arg = format!("{percent}%");
    match device.direction {
        DeviceDirection::Output | DeviceDirection::Duplex => {
            run_pactl(&["set-sink-volume", &device.system_name, &volume_arg])?;
            if uses_monitor_fan_out(device) {
                run_pactl(&[
                    "set-source-volume",
                    &monitor_source_name(&device.system_name),
                    &volume_arg,
                ])?;
            }
        }
        DeviceDirection::Input => {
            run_pactl(&[
                "set-source-volume",
                &device.system_name,
                &volume_arg,
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
            if uses_monitor_fan_out(device) {
                run_pactl(&[
                    "set-source-mute",
                    &monitor_source_name(&device.system_name),
                    flag,
                ])?;
            }
        }
        DeviceDirection::Input => {
            run_pactl(&["set-source-mute", &device.system_name, flag])?;
        }
    }
    Ok(())
}

pub fn set_stream_volume(
    graph: &RuntimeGraph,
    stream_id: &str,
    percent: u8,
) -> Result<(), AdapterError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| AdapterError::Message(format!("stream not found: {stream_id}")))?;

    let volume_arg = format!("{}%", percent.min(100));
    match stream.direction {
        StreamDirection::Playback => {
            let index = find_sink_input_index(graph, stream)?;
            run_pactl(&["set-sink-input-volume", &index.to_string(), &volume_arg])?;
        }
        StreamDirection::Capture => {
            let index = find_source_output_index(graph, stream)?;
            run_pactl(&[
                "set-source-output-volume",
                &index.to_string(),
                &volume_arg,
            ])?;
        }
    }
    Ok(())
}

pub fn set_stream_mute(
    graph: &RuntimeGraph,
    stream_id: &str,
    muted: bool,
) -> Result<(), AdapterError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| AdapterError::Message(format!("stream not found: {stream_id}")))?;

    let flag = if muted { "1" } else { "0" };
    match stream.direction {
        StreamDirection::Playback => {
            let index = find_sink_input_index(graph, stream)?;
            run_pactl(&["set-sink-input-mute", &index.to_string(), flag])?;
        }
        StreamDirection::Capture => {
            let index = find_source_output_index(graph, stream)?;
            run_pactl(&["set-source-output-mute", &index.to_string(), flag])?;
        }
    }
    Ok(())
}

/// Sets volume directly on a sink by its raw system/module name, bypassing
/// `RuntimeGraph` lookup. Used for per-mix-source feed sinks, which are
/// intentionally hidden from the graph's device list.
pub fn set_sink_volume_by_name(system_name: &str, percent: u8) -> Result<(), AdapterError> {
    let volume_arg = format!("{}%", percent.min(100));
    run_pactl(&["set-sink-volume", system_name, &volume_arg]).map(|_| ())
}

/// Reads the current volume of a sink by its raw system/module name. Used to
/// reflect a per-mix-source feed sink's gain back into the runtime graph.
pub fn sink_volume_percent(system_name: &str) -> Result<Option<u8>, AdapterError> {
    let output = run_pactl(&["list", "sinks"])?;
    let mut current_name = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Name: ") {
            current_name = Some(rest.trim().to_string());
            continue;
        }
        if line.starts_with("Volume:") && current_name.as_deref() == Some(system_name) {
            return Ok(super::parse::extract_volume_percent(line));
        }
    }

    Ok(None)
}

/// Mutes/unmutes a sink directly by its raw system/module name — the
/// counterpart to `set_sink_volume_by_name` for a per-mix-source feed sink.
/// Muting here never touches any `pw-link` connection: the feed sink stays
/// wired exactly as it was, only its own mute flag changes.
pub fn set_sink_mute_by_name(system_name: &str, muted: bool) -> Result<(), AdapterError> {
    let flag = if muted { "1" } else { "0" };
    run_pactl(&["set-sink-mute", system_name, flag]).map(|_| ())
}

/// Reads the current mute state of a sink by its raw system/module name.
pub fn sink_mute_state(system_name: &str) -> Result<Option<bool>, AdapterError> {
    let output = run_pactl(&["list", "sinks"])?;
    let mut current_name = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Name: ") {
            current_name = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Mute: ") {
            if current_name.as_deref() == Some(system_name) {
                return Ok(Some(rest.trim() == "yes"));
            }
        }
    }

    Ok(None)
}

fn uses_monitor_fan_out(device: &crate::core::models::Device) -> bool {
    device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Output
}

fn monitor_source_name(sink_system_name: &str) -> String {
    format!("{sink_system_name}.monitor")
}
