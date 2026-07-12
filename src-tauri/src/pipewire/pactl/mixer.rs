use crate::core::models::{DeviceDirection, DeviceKind, RuntimeGraph, StreamDirection};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl::parse::{find_sink_input_index, find_source_output_index};
use crate::pipewire::pactl::run_pactl;

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

fn uses_monitor_fan_out(device: &crate::core::models::Device) -> bool {
    device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Output
}

fn monitor_source_name(sink_system_name: &str) -> String {
    format!("{sink_system_name}.monitor")
}
