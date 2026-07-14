use crate::backend::BackendError;
use crate::backend::linux::pactl;
use crate::backend::linux::pw_link;
use crate::core::models::{DeviceDirection, RuntimeGraph};

/// Resolves a connection source (device or stream) to the system name used
/// for feed-sink naming/gain control, and whether it's a stream (needs
/// `pactl move-sink-input`) vs. a device (needs port-linking). Streams
/// without a stable `system_name` fall back to their runtime id — the
/// connection effect still works live, it just won't survive a PipeWire
/// restart the way a device-backed one does, since the persistence key would
/// no longer resolve to the same stream.
fn resolve_source(graph: &RuntimeGraph, source_id: &str) -> Result<(String, bool), BackendError> {
    if let Some(device) = graph.devices.iter().find(|device| device.id == source_id) {
        return Ok((device.system_name.clone(), false));
    }
    if let Some(stream) = graph.streams.iter().find(|stream| stream.id == source_id) {
        let name = stream.system_name.clone().unwrap_or_else(|| stream.id.clone());
        return Ok((name, true));
    }
    Err(BackendError::Message(format!("connection source not found: {source_id}")))
}

/// Inserts a per-connection feed sink between `source_id` and
/// `target_device_id`, rerouting the source through it and linking its
/// monitor to the target — same mechanism as `virtual_mic_mix`'s per-pair
/// feed sinks, generalized to any source (device or stream) and any target
/// device (not just a virtual mic). Returns the resolved `(source_system_name,
/// target_system_name)` pair so the caller can persist against it.
pub fn add_connection_effect(
    graph: &RuntimeGraph,
    source_id: &str,
    target_device_id: &str,
) -> Result<(String, String), BackendError> {
    let (source_system_name, source_is_stream) = resolve_source(graph, source_id)?;
    let target = graph
        .devices
        .iter()
        .find(|device| device.id == target_device_id)
        .ok_or_else(|| BackendError::Message(format!("target device not found: {target_device_id}")))?;
    let target_system_name = target.system_name.clone();

    let feed_name =
        pactl::ensure_feed_sink_for_connection(&source_system_name, &target_system_name, &target.label)?;

    if source_is_stream {
        pactl::move_stream_to_sink_name(graph, source_id, &feed_name)?;
    } else {
        pw_link::link_capture_source_to_sink(&source_system_name, &feed_name)?;
    }

    let target_is_virtual_source = target.direction == DeviceDirection::Input;
    pw_link::link_sink_monitor_to_target(&feed_name, &target_system_name, target_is_virtual_source)?;

    Ok((source_system_name, target_system_name))
}

/// Sets the gain for one already-inserted connection effect, without
/// touching linking — safe to call at high frequency for a live slider drag.
pub fn set_connection_volume(
    source_system_name: &str,
    target_system_name: &str,
    volume_percent: u8,
) -> Result<(), BackendError> {
    let feed_name = pactl::feed_sink_name_for_connection(source_system_name, target_system_name);
    pactl::set_sink_volume_by_name(&feed_name, volume_percent)
}

/// Mutes/unmutes a connection's feed sink directly — no relinking, so the
/// port connections (and this source's place in the routing) are untouched.
pub fn set_connection_mute(
    source_system_name: &str,
    target_system_name: &str,
    muted: bool,
) -> Result<(), BackendError> {
    let feed_name = pactl::feed_sink_name_for_connection(source_system_name, target_system_name);
    pactl::set_sink_mute_by_name(&feed_name, muted)
}

/// Tears down a connection's feed sink entirely, reverting to a direct
/// (ungained) route. Callers are responsible for re-establishing the plain
/// route afterward (e.g. via `route_stream`/`route_device`) if the
/// connection should still exist without an effect.
pub fn remove_connection_effect(
    source_system_name: &str,
    target_system_name: &str,
) -> Result<(), BackendError> {
    pactl::remove_feed_sink_for_connection(source_system_name, target_system_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Device, DeviceKind, Stream, StreamDirection};

    fn sample_graph() -> RuntimeGraph {
        RuntimeGraph {
            devices: vec![
                Device {
                    id: "mic-1".into(),
                    system_name: "alsa_input.headset".into(),
                    label: "Headset Mic".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Input,
                    sink_mode: None,
                    volume_percent: Some(100),
                    muted: Some(false),
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                },
                Device {
                    id: "speakers-1".into(),
                    system_name: "alsa_output.speakers".into(),
                    label: "Speakers".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: Some(100),
                    muted: Some(false),
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                },
            ],
            streams: vec![Stream {
                id: "stream-spotify".into(),
                app_name: "Spotify".into(),
                executable: None,
                window_class: None,
                system_name: None,
                direction: StreamDirection::Playback,
                current_target: None,
                current_targets: Vec::new(),
                media_name: None,
                is_system: false,
                volume_percent: Some(80),
                muted: Some(false),
                route_explanation: None,
            }],
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
            recent_stream_identities: Vec::new(),
        }
    }

    #[test]
    fn resolve_source_finds_device_by_id() {
        let graph = sample_graph();
        let (name, is_stream) = resolve_source(&graph, "mic-1").unwrap();
        assert_eq!(name, "alsa_input.headset");
        assert!(!is_stream);
    }

    #[test]
    fn resolve_source_falls_back_to_stream_id_without_a_stable_system_name() {
        let graph = sample_graph();
        let (name, is_stream) = resolve_source(&graph, "stream-spotify").unwrap();
        assert_eq!(name, "stream-spotify");
        assert!(is_stream);
    }

    #[test]
    fn resolve_source_errors_on_unknown_id() {
        let graph = sample_graph();
        let error = resolve_source(&graph, "not-a-real-id").expect_err("should error");
        assert!(error.to_string().contains("not found"));
    }
}
