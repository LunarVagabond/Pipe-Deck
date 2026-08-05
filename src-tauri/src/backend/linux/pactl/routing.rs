use crate::core::models::{DeviceDirection, DeviceKind, RuntimeGraph, StreamDirection};
use crate::backend::BackendError;
use crate::backend::linux::pactl::parse::{find_sink_input_index, find_source_output_index};
use crate::backend::linux::pactl::run_pactl;
use crate::backend::linux::pactl::r#virtual::{
    create_null_sink, create_virtual_source, ensure_feed_sink_for_virtual_input,
    feed_sink_name_for_virtual_input, sink_exists,
};
use crate::backend::linux::pw_link;
use crate::backend::linux::pw_metadata_native as native;
use std::collections::HashSet;
use crate::sysproc;

pub fn move_stream_to_target(
    graph: &RuntimeGraph,
    stream_id: &str,
    target_device_id: &str,
) -> Result<(), BackendError> {
    let target = graph
        .devices
        .iter()
        .find(|device| device.id == target_device_id)
        .ok_or_else(|| BackendError::Message(format!("target device not found: {target_device_id}")))?;

    move_stream_to_resolved_target(graph, stream_id, target)
}

pub fn move_stream_to_sink_name(
    graph: &RuntimeGraph,
    stream_id: &str,
    sink_system_name: &str,
) -> Result<(), BackendError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;

    if stream.direction != StreamDirection::Playback {
        return Err(BackendError::Message(
            "only playback streams can be moved to a sink".into(),
        ));
    }

    if let Some(stream_system_name) = stream.system_name.as_deref() {
        return pw_link::route_playback_stream(stream_system_name, sink_system_name);
    }

    // No live node name for this stream at all (structurally possible if
    // its `node.name` came back empty — see `pw_dump.rs`'s stream
    // normalization) — `pw_link::route_playback_stream` needs a name to link
    // against either natively or via `pw-link`, so this is the one case
    // that still needs the original `pactl` index-based move.
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
) -> Result<(), BackendError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;

    // Mirrors the pre-#428 CLI path's own short-circuit: if the stream's
    // already gone (a stale graph snapshot, or it ended between the caller
    // reading the graph and this call), do nothing rather than resolve a
    // fallback target — and possibly create the synthetic "Unrouted"
    // sink/source as a side effect — for a move that was never going to
    // happen anyway. Checked natively first (no `pactl` shellout needed to
    // answer "does this node still exist") with the original index-based
    // check as the fallback when native isn't available.
    if !stream_is_still_live(graph, stream) {
        return Ok(());
    }

    match stream.direction {
        StreamDirection::Playback => {
            let avoid = avoid_sink_system_names(graph, avoid_target_device_id);
            let fallback = resolve_clear_playback_sink(graph, &avoid)?;
            move_sink_input_with_fallback(graph, stream, &fallback)?;
        }
        StreamDirection::Capture => {
            let avoid = avoid_source_system_names(graph, avoid_target_device_id);
            let fallback = resolve_clear_capture_source(graph, &avoid)?;
            move_source_output_with_fallback(graph, stream, &fallback)?;
        }
    }

    Ok(())
}

fn stream_is_still_live(graph: &RuntimeGraph, stream: &crate::core::models::Stream) -> bool {
    if let Some(stream_system_name) = stream.system_name.as_deref() {
        return match stream.direction {
            StreamDirection::Playback => pw_link::has_output_ports(stream_system_name),
            StreamDirection::Capture => pw_link::has_input_ports(stream_system_name),
        };
    }

    match stream.direction {
        StreamDirection::Playback => find_sink_input_index(graph, stream).is_ok(),
        StreamDirection::Capture => find_source_output_index(graph, stream).is_ok(),
    }
}

fn move_sink_input_with_fallback(
    graph: &RuntimeGraph,
    stream: &crate::core::models::Stream,
    sink_name: &str,
) -> Result<(), BackendError> {
    if let Some(stream_system_name) = stream.system_name.as_deref() {
        if pw_link::route_playback_stream(stream_system_name, sink_name).is_ok() {
            return Ok(());
        }
        ensure_unrouted_playback_sink()?;
        return pw_link::route_playback_stream(stream_system_name, UNROUTED_PLAYBACK_SINK);
    }

    let index = match find_sink_input_index(graph, stream) {
        Ok(index) => index,
        Err(_) => return Ok(()),
    };
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

fn move_source_output_with_fallback(
    graph: &RuntimeGraph,
    stream: &crate::core::models::Stream,
    source_name: &str,
) -> Result<(), BackendError> {
    if let Some(stream_system_name) = stream.system_name.as_deref() {
        if pw_link::route_capture_stream(source_name, stream_system_name).is_ok() {
            return Ok(());
        }
        ensure_unrouted_capture_source()?;
        return pw_link::route_capture_stream(UNROUTED_CAPTURE_SOURCE, stream_system_name);
    }

    let index = match find_source_output_index(graph, stream) {
        Ok(index) => index,
        Err(_) => return Ok(()),
    };
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

fn ensure_unrouted_playback_sink() -> Result<(), BackendError> {
    if sink_exists(UNROUTED_PLAYBACK_SINK)? {
        return Ok(());
    }
    create_null_sink(UNROUTED_PLAYBACK_SINK, "Unrouted")?;
    Ok(())
}

fn ensure_unrouted_capture_source() -> Result<(), BackendError> {
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
) -> Result<String, BackendError> {
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
) -> Result<String, BackendError> {
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

pub(crate) fn get_default_sink_name() -> Option<String> {
    if let Some(result) = native::default_sink_name() {
        return result.ok().flatten();
    }
    read_pactl_default_name(&["get-default-sink"])
}

fn get_default_source_name() -> Option<String> {
    if let Some(result) = native::default_source_name() {
        return result.ok().flatten();
    }
    read_pactl_default_name(&["get-default-source"])
}

/// Public wrapper around [`get_default_sink_name`] (#11) — the tray's
/// default-output quick control needs to read the current default sink the
/// same way `resolve_clear_playback_sink` already does internally, rather
/// than re-implementing the native-metadata-then-pactl-fallback dance.
pub fn default_output_system_name() -> Option<String> {
    get_default_sink_name()
}

/// Sets `system_name` as the PipeWire/PulseAudio default sink (#11's tray
/// "switch default output" action). No native `pw::metadata` write path
/// exists yet (only the read side does, see `pw_metadata_native.rs`), so
/// this always shells out to `pactl set-default-sink`, mirroring how
/// `move_stream_to_sink_name` and friends already fall back to `pactl` for
/// state-changing routing operations elsewhere in this module.
pub fn set_default_output_system_name(system_name: &str) -> Result<(), BackendError> {
    crate::backend::linux::pactl::run_pactl(&["set-default-sink", system_name]).map(|_| ())
}

fn read_pactl_default_name(args: &[&str]) -> Option<String> {
    let output = sysproc::command("pactl").args(args).output().ok()?;
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
) -> Result<(), BackendError> {
    let stream = graph
        .streams
        .iter()
        .find(|stream| stream.id == stream_id)
        .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;

    match stream.direction {
        StreamDirection::Playback => {
            let sink_name = resolve_playback_sink_name(target)?;
            if !matches!(target.direction, DeviceDirection::Output | DeviceDirection::Duplex | DeviceDirection::Input) {
                return Err(BackendError::Message(
                    "playback streams must target an output or virtual input".into(),
                ));
            }
            if let Some(stream_system_name) = stream.system_name.as_deref() {
                return pw_link::route_playback_stream(stream_system_name, &sink_name);
            }
            let input_index = find_sink_input_index(graph, stream)?;
            run_pactl(&["move-sink-input", &input_index.to_string(), &sink_name])?;
        }
        StreamDirection::Capture => {
            if !matches!(target.direction, DeviceDirection::Input | DeviceDirection::Duplex) {
                return Err(BackendError::Message(
                    "capture streams must target an input device".into(),
                ));
            }
            if let Some(stream_system_name) = stream.system_name.as_deref() {
                return pw_link::route_capture_stream(&target.system_name, stream_system_name);
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

fn resolve_playback_sink_name(target: &crate::core::models::Device) -> Result<String, BackendError> {
    if target.direction == DeviceDirection::Input && target.kind == crate::core::models::DeviceKind::Virtual {
        let feed_sink = ensure_feed_sink_for_virtual_input(&target.system_name, &target.label)?;
        pw_link::link_sink_monitor_to_target(&feed_sink, &target.system_name, true)?;
        return Ok(feed_sink);
    }

    if !matches!(target.direction, DeviceDirection::Output | DeviceDirection::Duplex) {
        return Err(BackendError::Message(
            "playback streams must target an output device".into(),
        ));
    }

    Ok(target.system_name.clone())
}
