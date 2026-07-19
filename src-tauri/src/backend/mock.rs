use crate::core::models::{
    Device, DeviceDirection, DeviceKind, Link, MixSource, MixSourceSpec, RuntimeGraph, SinkMode,
    Stream, StreamDirection, VirtualDeviceInfo, VirtualDeviceResult,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use crate::backend::{BackendError, GraphListener, AudioBackend};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// Holds a mutable in-memory graph seeded from the static sample data, so
/// mixer/routing/virtual-mic-mix mutations actually persist across a
/// `fetch_graph()` call the way a real backend's live state would — unlike
/// the original stateless mock, which returned a fresh copy of the sample
/// data on every call and relied on `CoreEngine`'s own
/// `data_source == "mock"` branches to fake persistence in-place.
pub struct MockAudioBackend {
    graph: Mutex<RuntimeGraph>,
}

impl MockAudioBackend {
    pub fn new() -> Self {
        Self {
            graph: Mutex::new(Self::sample_graph()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RuntimeGraph> {
        self.graph.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn push_virtual_device(&self, label: &str, direction: DeviceDirection, multi: bool) -> VirtualDeviceResult {
        let slug = crate::backend::slugify(label);
        let system_name = format!("pipe-deck-{slug}");
        let device_id = format!("virtual-{slug}");
        let mut graph = self.lock();
        graph.devices.push(Device {
            id: device_id.clone(),
            system_name: system_name.clone(),
            label: label.to_string(),
            kind: DeviceKind::Virtual,
            direction: direction.clone(),
            sink_mode: match direction {
                DeviceDirection::Output | DeviceDirection::Duplex => {
                    Some(if multi { SinkMode::Multi } else { SinkMode::Single })
                }
                DeviceDirection::Input => None,
            },
            volume_percent: Some(100),
            muted: Some(false),
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        });
        VirtualDeviceResult {
            device_id,
            system_name,
            label: label.to_string(),
            multi,
        }
    }

    fn sample_graph() -> RuntimeGraph {
        RuntimeGraph {
            devices: vec![
                // `current_target`/`current_targets` below mirror the `links` list further
                // down verbatim — the port/handle-assignment code in
                // `nodePorts.ts::computeDeviceConnections` reads these fields (via
                // `deviceTargetIds`), not `graph.links` directly, so leaving them unset
                // (as `mock_device()` defaults them) makes Vue Flow unable to find the
                // named output handle and fall back to anchoring the edge at the node's
                // top — a real edge with no correct port, sample-data-only bug that a
                // live PipeWire graph never hits since these fields come from actual
                // routing state there.
                mock_device_routed(
                    "sink-chat",
                    "Chat",
                    DeviceKind::Virtual,
                    DeviceDirection::Output,
                    &["sink-headphones"],
                ),
                mock_device_routed(
                    "sink-music",
                    "Music",
                    DeviceKind::Virtual,
                    DeviceDirection::Output,
                    &["sink-headphones", "sink-stream-output"],
                ),
                mock_device_routed(
                    "sink-game",
                    "Game",
                    DeviceKind::Virtual,
                    DeviceDirection::Output,
                    &["sink-headphones"],
                ),
                mock_device_routed(
                    "sink-browser",
                    "Browser",
                    DeviceKind::Virtual,
                    DeviceDirection::Output,
                    &["sink-speakers"],
                ),
                mock_device_routed(
                    "sink-stream-mix",
                    "Stream Mix",
                    DeviceKind::Virtual,
                    DeviceDirection::Output,
                    &["sink-stream-output"],
                ),
                mock_device("sink-headphones", "Headphones", DeviceKind::Physical, DeviceDirection::Output),
                mock_device("sink-speakers", "Speakers", DeviceKind::Physical, DeviceDirection::Output),
                mock_device("sink-stream-output", "Stream Output", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("source-mic", "Microphone", DeviceKind::Physical, DeviceDirection::Input),
                mock_device_with_mix_sources(
                    "source-mic-filtered",
                    "Mic (Filtered)",
                    DeviceKind::Virtual,
                    DeviceDirection::Input,
                    &["source-mic"],
                ),
            ],
            streams: vec![
                Stream {
                    id: "stream-discord".into(),
                    app_name: "Discord".into(),
                    executable: Some("discord".into()),
                    window_class: None,
                    system_name: Some("stream-discord".into()),
                    direction: StreamDirection::Playback,
                    current_target: Some("sink-chat".into()),
                    media_name: None,
                    is_system: false,
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                },
                Stream {
                    id: "stream-spotify".into(),
                    app_name: "Spotify".into(),
                    executable: Some("spotify".into()),
                    window_class: None,
                    system_name: Some("stream-spotify".into()),
                    direction: StreamDirection::Playback,
                    current_target: Some("sink-music".into()),
                    media_name: None,
                    is_system: false,
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                },
                Stream {
                    id: "stream-steam".into(),
                    app_name: "Steam".into(),
                    executable: Some("steam".into()),
                    window_class: None,
                    system_name: Some("stream-steam".into()),
                    direction: StreamDirection::Playback,
                    current_target: Some("sink-game".into()),
                    media_name: None,
                    is_system: false,
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                },
                Stream {
                    id: "stream-firefox".into(),
                    app_name: "Firefox".into(),
                    executable: Some("firefox".into()),
                    window_class: None,
                    system_name: Some("stream-firefox".into()),
                    direction: StreamDirection::Playback,
                    current_target: Some("sink-browser".into()),
                    media_name: None,
                    is_system: false,
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                },
                Stream {
                    id: "stream-obs".into(),
                    app_name: "OBS".into(),
                    executable: Some("obs".into()),
                    window_class: None,
                    system_name: Some("stream-obs".into()),
                    direction: StreamDirection::Capture,
                    current_target: Some("source-mic-filtered".into()),
                    media_name: None,
                    is_system: false,
                    volume_percent: None,
                    muted: None,
                    route_explanation: None,
                },
            ],
            links: vec![
                // Apps → virtual sinks
                Link {
                    id: "link-discord-chat".into(),
                    source_id: "stream-discord".into(),
                    target_id: "sink-chat".into(),
                },
                Link {
                    id: "link-spotify-music".into(),
                    source_id: "stream-spotify".into(),
                    target_id: "sink-music".into(),
                },
                Link {
                    id: "link-steam-game".into(),
                    source_id: "stream-steam".into(),
                    target_id: "sink-game".into(),
                },
                Link {
                    id: "link-firefox-browser".into(),
                    source_id: "stream-firefox".into(),
                    target_id: "sink-browser".into(),
                },
                // Virtual sinks → outputs
                Link {
                    id: "link-chat-headphones".into(),
                    source_id: "sink-chat".into(),
                    target_id: "sink-headphones".into(),
                },
                Link {
                    id: "link-music-headphones".into(),
                    source_id: "sink-music".into(),
                    target_id: "sink-headphones".into(),
                },
                Link {
                    id: "link-music-stream".into(),
                    source_id: "sink-music".into(),
                    target_id: "sink-stream-output".into(),
                },
                Link {
                    id: "link-game-headphones".into(),
                    source_id: "sink-game".into(),
                    target_id: "sink-headphones".into(),
                },
                Link {
                    id: "link-browser-speakers".into(),
                    source_id: "sink-browser".into(),
                    target_id: "sink-speakers".into(),
                },
                Link {
                    id: "link-stream-mix-output".into(),
                    source_id: "sink-stream-mix".into(),
                    target_id: "sink-stream-output".into(),
                },
                // Capture path — source is the device feeding the mic-filtered audio out
                // to OBS's capture input, not the other way around; this must match the
                // direction `stream-obs.current_target` and `handlesForStream`/
                // `computeDeviceConnections` already encode, or neither edge endpoint
                // resolves to a real handle on either node (see the sibling comment on
                // `sample_graph`'s devices list).
                Link {
                    id: "link-obs-mic".into(),
                    source_id: "source-mic-filtered".into(),
                    target_id: "stream-obs".into(),
                },
                Link {
                    id: "link-mic-filtered".into(),
                    source_id: "source-mic".into(),
                    target_id: "source-mic-filtered".into(),
                },
            ],
            data_source: "mock".into(),
            notice: Some(
                "Sample data only. Unset PIPE_DECK_USE_MOCK to use live PipeWire.".into(),
            ),
            ..Default::default()
        }
    }
}

fn mock_device(
    id: &str,
    label: &str,
    kind: DeviceKind,
    direction: DeviceDirection,
) -> Device {
    Device {
        id: id.into(),
        system_name: id.into(),
        label: label.into(),
        kind,
        direction,
        sink_mode: None,
        volume_percent: Some(70),
        muted: Some(false),
        current_target: None,
        current_targets: Vec::new(),
        mix_sources: Vec::new(),
    }
}

/// Same as `mock_device`, but with `current_target`/`current_targets` set —
/// for virtual-sink devices whose fan-out the `links` list below also
/// describes; keep the two in sync (see the comment on `sample_graph`).
fn mock_device_routed(
    id: &str,
    label: &str,
    kind: DeviceKind,
    direction: DeviceDirection,
    targets: &[&str],
) -> Device {
    Device {
        current_target: targets.first().map(|target| (*target).into()),
        current_targets: targets.iter().map(|target| (*target).into()).collect(),
        ..mock_device(id, label, kind, direction)
    }
}

/// Same as `mock_device`, but with `mix_sources` set — for a virtual input
/// device whose mic-mix merge the `links` list below also describes.
fn mock_device_with_mix_sources(
    id: &str,
    label: &str,
    kind: DeviceKind,
    direction: DeviceDirection,
    source_device_ids: &[&str],
) -> Device {
    Device {
        mix_sources: source_device_ids
            .iter()
            .map(|device_id| MixSource {
                device_id: (*device_id).into(),
                volume_percent: 100,
                muted: false,
            })
            .collect(),
        ..mock_device(id, label, kind, direction)
    }
}

impl AudioBackend for MockAudioBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        Ok(self.lock().clone())
    }

    fn subscribe(&self, _listener: GraphListener) -> Result<(), BackendError> {
        // Mock adapter is static; real PipeWire adapter will push live updates here.
        Ok(())
    }

    fn set_device_volume(&self, _graph: &RuntimeGraph, device_id: &str, percent: u8) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let device = graph
            .devices
            .iter_mut()
            .find(|device| device.id == device_id)
            .ok_or_else(|| BackendError::Message(format!("device not found: {device_id}")))?;
        device.volume_percent = Some(percent.min(100));
        Ok(())
    }

    fn set_device_mute(&self, _graph: &RuntimeGraph, device_id: &str, muted: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let device = graph
            .devices
            .iter_mut()
            .find(|device| device.id == device_id)
            .ok_or_else(|| BackendError::Message(format!("device not found: {device_id}")))?;
        device.muted = Some(muted);
        Ok(())
    }

    fn set_stream_volume(&self, _graph: &RuntimeGraph, stream_id: &str, percent: u8) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let stream = graph
            .streams
            .iter_mut()
            .find(|stream| stream.id == stream_id)
            .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;
        stream.volume_percent = Some(percent.min(100));
        Ok(())
    }

    fn set_stream_mute(&self, _graph: &RuntimeGraph, stream_id: &str, muted: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let stream = graph
            .streams
            .iter_mut()
            .find(|stream| stream.id == stream_id)
            .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;
        stream.muted = Some(muted);
        Ok(())
    }

    fn clear_stream_target(
        &self,
        _graph: &RuntimeGraph,
        stream_id: &str,
        _previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let stream = graph
            .streams
            .iter_mut()
            .find(|stream| stream.id == stream_id)
            .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;
        stream.current_target = None;
        Ok(())
    }

    fn route_stream(&self, _graph: &RuntimeGraph, stream_id: &str, target_device_id: &str) -> Result<(), BackendError> {
        let mut graph = self.lock();
        if !graph.devices.iter().any(|device| device.id == target_device_id) {
            return Err(BackendError::Message(format!("target device not found: {target_device_id}")));
        }
        let stream = graph
            .streams
            .iter_mut()
            .find(|stream| stream.id == stream_id)
            .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;
        stream.current_target = Some(target_device_id.to_string());
        Ok(())
    }

    fn route_device(&self, _graph: &RuntimeGraph, source_device_id: &str, target_device_ids: &[String]) -> Result<(), BackendError> {
        let mut graph = self.lock();
        if !graph.devices.iter().any(|device| device.id == source_device_id) {
            return Err(BackendError::Message(format!("source device not found: {source_device_id}")));
        }
        for target_id in target_device_ids {
            if !graph.devices.iter().any(|device| device.id == *target_id) {
                return Err(BackendError::Message(format!("target device not found: {target_id}")));
            }
        }
        let device = graph
            .devices
            .iter_mut()
            .find(|device| device.id == source_device_id)
            .expect("source device presence checked above");
        device.current_targets = target_device_ids.to_vec();
        device.current_target = target_device_ids.first().cloned();
        Ok(())
    }

    // The mock sample graph has no real pactl/pw-link session behind it, so
    // reconciliation that requires live PipeWire queries is a deliberate
    // no-op rather than shelling out to system tools with nothing meaningful
    // to report.
    fn sync_live_routing_graph(&self, _graph: &mut RuntimeGraph) {}

    fn apply_user_cleared_routes(
        &self,
        graph: &mut RuntimeGraph,
        cleared_streams: &HashSet<StreamIdentityKey>,
        cleared_devices: &HashSet<String>,
    ) {
        crate::backend::linux::graph_routing::apply_user_cleared_routes(
            graph,
            cleared_streams,
            cleared_devices,
        );
    }

    fn apply_graph_routing(&self, _graph: &mut RuntimeGraph, _ctx: &ApplyRulesContext<'_>) {}

    fn apply_virtual_mic_mix(&self, virtual_input: &Device, mix_sources: &[MixSourceSpec]) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let resolved: Vec<MixSource> = mix_sources
            .iter()
            .filter_map(|spec| {
                graph
                    .devices
                    .iter()
                    .find(|device| device.system_name == spec.system_name)
                    .map(|device| MixSource {
                        device_id: device.id.clone(),
                        volume_percent: spec.volume_percent,
                        muted: spec.muted,
                    })
            })
            .collect();
        if let Some(device) = graph.devices.iter_mut().find(|device| device.id == virtual_input.id) {
            device.mix_sources = resolved;
        }
        Ok(())
    }

    fn set_mix_source_volume(&self, virtual_input_system_name: &str, source_system_name: &str, percent: u8) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let source_device_id = graph
            .devices
            .iter()
            .find(|device| device.system_name == source_system_name)
            .map(|device| device.id.clone());
        if let Some(source_device_id) = source_device_id {
            if let Some(device) = graph
                .devices
                .iter_mut()
                .find(|device| device.system_name == virtual_input_system_name)
            {
                if let Some(mix_source) = device
                    .mix_sources
                    .iter_mut()
                    .find(|mix_source| mix_source.device_id == source_device_id)
                {
                    mix_source.volume_percent = percent;
                }
            }
        }
        Ok(())
    }

    fn set_mix_source_mute(&self, virtual_input_system_name: &str, source_system_name: &str, muted: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let source_device_id = graph
            .devices
            .iter()
            .find(|device| device.system_name == source_system_name)
            .map(|device| device.id.clone());
        if let Some(source_device_id) = source_device_id {
            if let Some(device) = graph
                .devices
                .iter_mut()
                .find(|device| device.system_name == virtual_input_system_name)
            {
                if let Some(mix_source) = device
                    .mix_sources
                    .iter_mut()
                    .find(|mix_source| mix_source.device_id == source_device_id)
                {
                    mix_source.muted = muted;
                }
            }
        }
        Ok(())
    }

    fn disconnect_all_virtual_mic_mixes(&self, virtual_input_system_name: &str) -> Result<(), BackendError> {
        let mut graph = self.lock();
        if let Some(device) = graph
            .devices
            .iter_mut()
            .find(|device| device.system_name == virtual_input_system_name)
        {
            device.mix_sources.clear();
        }
        Ok(())
    }

    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]) {
        crate::backend::linux::graph_enrich::apply_device_aliases(devices);
    }

    fn create_virtual_output(&self, label: &str, multi: bool) -> Result<VirtualDeviceResult, BackendError> {
        Ok(self.push_virtual_device(label, DeviceDirection::Output, multi))
    }

    fn create_virtual_input(&self, label: &str) -> Result<VirtualDeviceResult, BackendError> {
        Ok(self.push_virtual_device(label, DeviceDirection::Input, false))
    }

    fn restore_virtual_device(
        &self,
        system_name: &str,
        label: &str,
        direction: DeviceDirection,
        multi: bool,
        _mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError> {
        // Unreachable in practice: restore_session/restore_profile_virtual_devices/
        // apply_persisted_routes all short-circuit on PIPE_DECK_USE_MOCK=1
        // before ever calling this. Implemented for trait completeness.
        let mut graph = self.lock();
        graph.devices.push(Device {
            id: format!("virtual-{}", system_name.trim_start_matches("pipe-deck-")),
            system_name: system_name.to_string(),
            label: label.to_string(),
            kind: DeviceKind::Virtual,
            direction: direction.clone(),
            sink_mode: match direction {
                DeviceDirection::Output | DeviceDirection::Duplex => {
                    Some(if multi { SinkMode::Multi } else { SinkMode::Single })
                }
                DeviceDirection::Input => None,
            },
            volume_percent: Some(100),
            muted: Some(false),
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        });
        Ok(())
    }

    fn remove_virtual_device(&self, system_name: &str) -> Result<(), BackendError> {
        self.lock().devices.retain(|device| device.system_name != system_name);
        Ok(())
    }

    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo> {
        self.lock()
            .devices
            .iter()
            .filter(|device| device.kind == DeviceKind::Virtual)
            .map(|device| VirtualDeviceInfo {
                device_id: device.id.clone(),
                system_name: device.system_name.clone(),
                label: device.label.clone(),
                direction: device.direction.clone(),
                multi: device.sink_mode == Some(SinkMode::Multi),
            })
            .collect()
    }

    fn set_virtual_device_alias(&self, system_name: &str, alias: &str) -> Result<(), BackendError> {
        if let Some(device) = self.lock().devices.iter_mut().find(|device| device.system_name == system_name) {
            device.label = alias.to_string();
        }
        Ok(())
    }

    fn platform_audio_version(&self) -> Option<String> {
        Some("1.0.0 (mock)".to_string())
    }

    fn swap_to_effect_chain(
        &self,
        _device: &Device,
        _conf_path: &Path,
        _rendered_conf: &str,
        _downstream_targets: &[Device],
        _mic_feeders: &[String],
    ) -> Result<(), BackendError> {
        Ok(())
    }

    fn revert_to_plain_device(&self, _device: &Device, _wait_for_node: bool) -> Result<(), BackendError> {
        Ok(())
    }

    fn hold_sink_inputs_for_swap(&self, _device_system_name: &str) -> Result<Vec<u32>, BackendError> {
        Ok(Vec::new())
    }

    fn release_held_sink_inputs(&self, _held_indices: &[u32], _target_system_name: &str) -> Result<(), BackendError> {
        Ok(())
    }

    fn list_mic_feeds(&self, _target_system_name: &str, _target_is_virtual_source: bool) -> Vec<String> {
        Vec::new()
    }

    fn relink_mic_feeds(
        &self,
        _feeders: &[String],
        _from_system_name: &str,
        _to_system_name: &str,
        _to_is_virtual_source: bool,
    ) -> Result<(), BackendError> {
        Ok(())
    }
}
