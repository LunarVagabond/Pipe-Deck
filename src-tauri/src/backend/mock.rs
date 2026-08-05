use crate::core::models::{
    Device, DeviceDirection, DeviceKind, EffectChainConfig, LatencyHop, LatencyPathNode, LatencyPingResult, Link,
    MixSource, MixSourceSpec, PortDirection, ProcessingNode, ProcessingNodeKind, RuntimeGraph, SinkMode, Stream,
    StreamDirection, VirtualDeviceInfo, VirtualDeviceResult,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use crate::backend::{BackendError, GraphListener, AudioBackend};
use std::collections::HashSet;
use std::sync::Mutex;

/// Holds a mutable in-memory graph seeded from the static sample data, so
/// mixer/routing/virtual-mic-mix mutations actually persist across a
/// `fetch_graph()` call the way a real backend's live state would — unlike
/// the original stateless mock, which returned a fresh copy of the sample
/// data on every call and relied on `CoreEngine`'s own
/// `data_source == "mock"` branches to fake persistence in-place.
pub struct MockAudioBackend {
    graph: Mutex<RuntimeGraph>,
    /// system_names with a live effect chain "loaded" — tracked so
    /// `is_effect_chain_loaded` reflects real load/unload calls instead of
    /// always answering `false`, the same way `graph` makes routing/mixer
    /// mutations persist across a `fetch_graph()` the way a real backend's
    /// live state would.
    loaded_effect_chains: Mutex<HashSet<String>>,
    /// Same reasoning as `loaded_effect_chains`, for processing nodes
    /// (PD-032) — tracked so `is_processing_node_loaded` reflects real
    /// load/unload calls instead of always answering `false`.
    loaded_processing_nodes: Mutex<HashSet<String>>,
    /// `(path, target_system_name)` for every `play_sound` call (#393) — no
    /// trait query for this exists yet, so tests assert against this field
    /// directly rather than through the trait.
    played_sounds: Mutex<Vec<(std::path::PathBuf, String, u8)>>,
    /// Number of `stop_sound` calls (#399) — no tracked process exists to
    /// actually kill in the mock, so this just records that a stop was
    /// requested for tests to assert against.
    soundboard_stop_calls: Mutex<u32>,
    /// system_name of the current default output device (#11) — seeded to
    /// the first Output/Duplex device in the starting graph so
    /// `default_output_device_name` has something to answer immediately,
    /// same as a real PipeWire session always having *some* default sink.
    default_output: Mutex<Option<String>>,
}

impl Default for MockAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAudioBackend {
    pub fn new() -> Self {
        Self::with_graph(Self::sample_graph())
    }

    /// The actual `create_backend()` entry point (issue #368): loads a
    /// scenario from `PIPE_DECK_MOCK_SCENARIO` if set, falling back to the
    /// default `sample_graph()` on a missing/invalid path — a typo'd
    /// scenario path shouldn't block using Mock Mode at all, it should just
    /// log and behave like the env var was unset. Kept separate from
    /// `new()`, which the ~100 existing unit tests across the crate call
    /// expecting the default sample graph unconditionally; making `new()`
    /// itself env-sensitive would make all of them flaky under any test
    /// that happens to set `PIPE_DECK_MOCK_SCENARIO` and run in parallel.
    pub fn from_env() -> Self {
        let graph = match std::env::var("PIPE_DECK_MOCK_SCENARIO") {
            Ok(path) if !path.is_empty() => {
                match crate::backend::scenario::load_scenario_file(std::path::Path::new(&path)) {
                    Ok(graph) => graph,
                    Err(error) => {
                        eprintln!(
                            "PIPE_DECK_MOCK_SCENARIO={path}: {error} — falling back to the default sample graph"
                        );
                        Self::sample_graph()
                    }
                }
            }
            _ => Self::sample_graph(),
        };
        Self::with_graph(graph)
    }

    fn with_graph(graph: RuntimeGraph) -> Self {
        let default_output = graph
            .devices
            .iter()
            .find(|device| {
                device.kind != DeviceKind::Virtual
                    && matches!(device.direction, DeviceDirection::Output | DeviceDirection::Duplex)
            })
            .map(|device| device.system_name.clone());
        Self {
            graph: Mutex::new(graph),
            loaded_effect_chains: Mutex::new(HashSet::new()),
            loaded_processing_nodes: Mutex::new(HashSet::new()),
            played_sounds: Mutex::new(Vec::new()),
            soundboard_stop_calls: Mutex::new(0),
            default_output: Mutex::new(default_output),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RuntimeGraph> {
        self.graph.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn push_virtual_device(
        &self,
        label: &str,
        direction: DeviceDirection,
        multi: bool,
    ) -> VirtualDeviceResult {
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
            sample_rate: None,
            channels: None,
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
                    sample_rate: None,
                    channels: None,
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
                    sample_rate: None,
                    channels: None,
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
                    sample_rate: None,
                    channels: None,
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
                    sample_rate: None,
                    channels: None,
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
                    sample_rate: None,
                    channels: None,
                },
            ],
            links: vec![
                // Apps → virtual sinks
                Link {
                    id: "link-discord-chat".into(),
                    source_id: "stream-discord".into(),
                    target_id: "sink-chat".into(),
                    is_monitor: false,
                },
                Link {
                    id: "link-spotify-music".into(),
                    source_id: "stream-spotify".into(),
                    target_id: "sink-music".into(),
                    is_monitor: false,
                },
                Link {
                    id: "link-steam-game".into(),
                    source_id: "stream-steam".into(),
                    target_id: "sink-game".into(),
                    is_monitor: false,
                },
                Link {
                    id: "link-firefox-browser".into(),
                    source_id: "stream-firefox".into(),
                    target_id: "sink-browser".into(),
                    is_monitor: false,
                },
                // Virtual sinks → outputs
                Link {
                    id: "link-chat-headphones".into(),
                    source_id: "sink-chat".into(),
                    target_id: "sink-headphones".into(),
                    is_monitor: true,
                },
                Link {
                    id: "link-music-headphones".into(),
                    source_id: "sink-music".into(),
                    target_id: "sink-headphones".into(),
                    is_monitor: true,
                },
                Link {
                    id: "link-music-stream".into(),
                    source_id: "sink-music".into(),
                    target_id: "sink-stream-output".into(),
                    is_monitor: true,
                },
                Link {
                    id: "link-game-headphones".into(),
                    source_id: "sink-game".into(),
                    target_id: "sink-headphones".into(),
                    is_monitor: true,
                },
                Link {
                    id: "link-browser-speakers".into(),
                    source_id: "sink-browser".into(),
                    target_id: "sink-speakers".into(),
                    is_monitor: true,
                },
                Link {
                    id: "link-stream-mix-output".into(),
                    source_id: "sink-stream-mix".into(),
                    target_id: "sink-stream-output".into(),
                    is_monitor: true,
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
                    is_monitor: false,
                },
                Link {
                    id: "link-mic-filtered".into(),
                    source_id: "source-mic".into(),
                    target_id: "source-mic-filtered".into(),
                    is_monitor: false,
                },
            ],
            data_source: "mock".into(),
            notice: Some(
                "Sample data only. Unset PIPE_DECK_USE_MOCK to use live PipeWire.".into(),
            ),
            default_output_system_name: Some("sink-headphones".into()),
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
        sample_rate: None,
        channels: None,
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

    fn default_output_device_name(&self) -> Option<String> {
        self.default_output.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    fn set_default_output_device(&self, system_name: &str) -> Result<(), BackendError> {
        let graph = self.lock();
        let exists = graph.devices.iter().any(|device| {
            device.system_name == system_name
                && matches!(device.direction, DeviceDirection::Output | DeviceDirection::Duplex)
        });
        if !exists {
            return Err(BackendError::Message(format!(
                "output device not found: {system_name}"
            )));
        }
        drop(graph);
        *self.default_output.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(system_name.to_string());
        Ok(())
    }

    fn clear_stream_target(
        &self,
        _graph: &RuntimeGraph,
        stream_id: &str,
        avoid_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let direction = graph
            .streams
            .iter()
            .find(|stream| stream.id == stream_id)
            .map(|stream| stream.direction.clone())
            .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;

        // Mirrors the live backend's `resolve_clear_playback_sink`/
        // `resolve_clear_capture_source` fallback (minus the live default-sink
        // lookup, which has no mock equivalent): land on the first remaining
        // eligible device rather than stranding the stream with no target —
        // needed for `remove_virtual_device` (issue #208) to reroute streams
        // away from a device that's about to disappear.
        let fallback_id = graph
            .devices
            .iter()
            .find(|device| {
                if Some(device.id.as_str()) == avoid_target_device_id {
                    return false;
                }
                match direction {
                    StreamDirection::Playback => {
                        !(device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Input)
                            && matches!(device.direction, DeviceDirection::Output | DeviceDirection::Duplex)
                    }
                    StreamDirection::Capture => {
                        matches!(device.direction, DeviceDirection::Input | DeviceDirection::Duplex)
                    }
                }
            })
            .map(|device| device.id.clone());

        let stream = graph
            .streams
            .iter_mut()
            .find(|stream| stream.id == stream_id)
            .ok_or_else(|| BackendError::Message(format!("stream not found: {stream_id}")))?;
        stream.current_target = fallback_id;
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

    /// Trait default always returns `false`, which was fine while nothing
    /// tested against it — `core::routing::verify_route_applied` (issue
    /// #150) polls this to confirm a route actually took, so a real
    /// implementation is needed to exercise that function's success path
    /// against the mock. Resolves both device/stream ids to `system_name`
    /// on the fly since `current_target`/`current_targets` store the former
    /// but this trait method (like the live backend) is addressed by the
    /// latter.
    fn is_routed_to(&self, source_system_name: &str, target_system_name: &str, _target_is_input: bool) -> bool {
        let graph = self.lock();
        let target_ids: Vec<&str> = graph
            .devices
            .iter()
            .filter(|device| device.system_name == target_system_name)
            .map(|device| device.id.as_str())
            .collect();
        if target_ids.is_empty() {
            return false;
        }

        let source_targets: Vec<String> = if let Some(device) =
            graph.devices.iter().find(|device| device.system_name == source_system_name)
        {
            if !device.current_targets.is_empty() {
                device.current_targets.clone()
            } else {
                device.current_target.clone().into_iter().collect()
            }
        } else if let Some(stream) = graph
            .streams
            .iter()
            .find(|stream| stream.system_name.as_deref() == Some(source_system_name))
        {
            stream.current_target.clone().into_iter().collect()
        } else {
            Vec::new()
        };

        source_targets.iter().any(|target_id| target_ids.contains(&target_id.as_str()))
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

    fn create_virtual_output(
        &self,
        label: &str,
        multi: bool,
    ) -> Result<VirtualDeviceResult, BackendError> {
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
            sample_rate: None,
            channels: None,
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

    fn play_sound(&self, path: &std::path::Path, target_system_name: &str, volume_percent: u8) -> Result<(), BackendError> {
        self.played_sounds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((path.to_path_buf(), target_system_name.to_string(), volume_percent));
        Ok(())
    }

    fn stop_sound(&self) -> Result<(), BackendError> {
        *self.soundboard_stop_calls.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) += 1;
        Ok(())
    }

    fn platform_audio_version(&self) -> Option<String> {
        Some("1.0.0 (mock)".to_string())
    }

    /// Synthetic latency data — every node currently present in the mock
    /// graph (device, stream, or processing node) reports a fixed 1024/48000
    /// quantum/rate (~21.33ms), so mixer/routing tests get deterministic,
    /// non-zero data without shelling out to a real `pw-top`. A path node
    /// that isn't in the current graph reports no data, exercising the same
    /// "gap in the path" case a real backend would hit for a suspended node.
    fn measure_latency_ping(&self, path: &[LatencyPathNode]) -> Result<LatencyPingResult, BackendError> {
        const MOCK_QUANTUM: u32 = 1024;
        const MOCK_RATE: u32 = 48000;

        let graph = self.lock();
        let known_ids: HashSet<&str> = graph
            .devices
            .iter()
            .map(|device| device.id.as_str())
            .chain(graph.streams.iter().map(|stream| stream.id.as_str()))
            .chain(graph.processing_nodes.iter().map(|node| node.id.as_str()))
            .collect();

        let mut hops = Vec::with_capacity(path.len());
        let mut total_latency_ms = Some(0.0_f64);

        for node in path {
            let is_known = known_ids.contains(node.id.as_str());
            let (quantum, rate, latency_ms) = if is_known {
                let latency_ms = f64::from(MOCK_QUANTUM) / f64::from(MOCK_RATE) * 1000.0;
                (Some(MOCK_QUANTUM), Some(MOCK_RATE), Some(latency_ms))
            } else {
                (None, None, None)
            };

            match (total_latency_ms, latency_ms) {
                (Some(total), Some(hop_ms)) => total_latency_ms = Some(total + hop_ms),
                (_, None) => total_latency_ms = None,
                _ => {}
            }

            hops.push(LatencyHop {
                id: node.id.clone(),
                node_id: None,
                quantum,
                rate,
                latency_ms,
            });
        }

        Ok(LatencyPingResult { hops, total_latency_ms })
    }

    fn load_effect_chain(
        &self,
        device: &Device,
        _config: &EffectChainConfig,
        _downstream_targets: &[Device],
        _mic_feeders: &[String],
    ) -> Result<String, BackendError> {
        self.loaded_effect_chains
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(device.system_name.clone());
        Ok(format!("effect_output.{}", device.system_name))
    }

    fn unload_effect_chain(&self, device_system_name: &str) -> Result<(), BackendError> {
        self.loaded_effect_chains
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(device_system_name);
        Ok(())
    }

    fn is_effect_chain_loaded(&self, device_system_name: &str) -> bool {
        self.loaded_effect_chains
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(device_system_name)
    }

    fn load_processing_node(&self, node: &ProcessingNode) -> Result<(), BackendError> {
        self.loaded_processing_nodes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(node.system_name.clone());
        // Mock has no config-backed merge step the way the real Linux
        // backend does (`processing_node_ops::merge_processing_nodes`) — its
        // own graph *is* the source of truth, so existence lives here
        // directly, the same way `push_virtual_device` is how a mock device
        // comes to exist at all.
        let mut graph = self.lock();
        if let Some(existing) = graph.processing_nodes.iter_mut().find(|existing| existing.system_name == node.system_name) {
            *existing = node.clone();
        } else {
            graph.processing_nodes.push(node.clone());
        }
        Ok(())
    }

    fn unload_processing_node(&self, system_name: &str) -> Result<(), BackendError> {
        self.loaded_processing_nodes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(system_name);
        self.lock().processing_nodes.retain(|node| node.system_name != system_name);
        Ok(())
    }

    fn is_processing_node_loaded(&self, system_name: &str) -> bool {
        self.loaded_processing_nodes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(system_name)
    }

    fn relink_processing_node_port(
        &self,
        _graph: &RuntimeGraph,
        system_name: &str,
        port_index: u32,
        direction: PortDirection,
        peer_id: Option<&str>,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        let ports = match direction {
            PortDirection::Input => &mut node.inputs,
            PortDirection::Output => &mut node.outputs,
        };
        // Tracks what actually happened to the port list so a Mixer's
        // per-input gain array (a second, parallel `Vec` on `node.kind`, not
        // part of `ProcessingNodePort` itself) can be kept in exact lockstep
        // — grown/removed at the *same* index, not just resized to the same
        // final length, which would silently shift every later input's gain
        // down by one instead of dropping the disconnected one specifically.
        enum PortEdit {
            Grew,
            RemovedAt(usize),
            Unchanged,
        }
        let edit = match peer_id {
            Some(peer) => match ports.iter_mut().find(|port| port.index == port_index) {
                Some(port) => {
                    port.connected_id = Some(peer.to_string());
                    PortEdit::Unchanged
                }
                // A fresh port one past the current end — this is how a
                // Mixer's inputs / Fan-out's outputs grow with each
                // connection (see `CoreEngine::connect_processing_node_port`).
                None => {
                    ports.push(crate::core::models::ProcessingNodePort {
                        index: port_index,
                        connected_id: Some(peer.to_string()),
                        feed_key: None,
                    });
                    PortEdit::Grew
                }
            },
            // Disconnect removes the port entirely and re-indexes what's
            // left, rather than leaving a hole — matches how the real
            // backend's persisted `output_targets`/`input_sources` (plain
            // `Vec`s, re-derived by position on every merge) behave.
            None => match ports.iter().position(|port| port.index == port_index) {
                Some(position) => {
                    ports.remove(position);
                    for port in ports.iter_mut() {
                        if port.index > port_index {
                            port.index -= 1;
                        }
                    }
                    PortEdit::RemovedAt(position)
                }
                None => PortEdit::Unchanged,
            },
        };
        if direction == PortDirection::Input {
            if let ProcessingNodeKind::Mixer { input_gains_percent } = &mut node.kind {
                match edit {
                    PortEdit::Grew => input_gains_percent.push(100),
                    PortEdit::RemovedAt(position) => {
                        if position < input_gains_percent.len() {
                            input_gains_percent.remove(position);
                        }
                    }
                    PortEdit::Unchanged => {}
                }
            }
        }
        Ok(())
    }

    fn set_processing_node_input_gain(
        &self,
        system_name: &str,
        peer_system_name: &str,
        gain_percent: u8,
        _muted: bool,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        // `ProcessingNodePort.connected_id` holds a device/stream *id*, not a
        // system name — resolve the other way round first, same as the real
        // Linux backend receives a device/stream id from the engine and
        // resolves it against `graph` itself.
        let peer_id = graph
            .devices
            .iter()
            .find(|device| device.system_name == peer_system_name)
            .map(|device| device.id.clone());
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        let Some(port_index) = peer_id
            .and_then(|id| node.inputs.iter().find(|port| port.connected_id.as_deref() == Some(id.as_str())).map(|port| port.index))
            .map(|index| index as usize)
        else {
            return Err(BackendError::Message(format!("input not connected: {peer_system_name}")));
        };
        if let ProcessingNodeKind::Mixer { input_gains_percent } = &mut node.kind {
            if let Some(slot) = input_gains_percent.get_mut(port_index) {
                *slot = gain_percent;
            }
        }
        Ok(())
    }

    fn set_processing_node_volume(&self, system_name: &str, volume_percent: u8, muted: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        match &node.kind {
            ProcessingNodeKind::FanOut { .. } => {
                node.kind = ProcessingNodeKind::FanOut { volume_percent, muted };
            }
            ProcessingNodeKind::Group { .. } => {
                node.kind = ProcessingNodeKind::Group { volume_percent, muted };
            }
            _ => {}
        }
        Ok(())
    }

    fn set_processing_node_eq_params(
        &self,
        system_name: &str,
        eq_sub: i32,
        eq_bass: i32,
        eq_mid: i32,
        eq_treble: i32,
        eq_air: i32,
        output_gain: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Eq5Band { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Eq5Band { eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn set_processing_node_delay_params(
        &self,
        system_name: &str,
        delay_ms: i32,
        feedback_percent: i32,
        feedforward_percent: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Delay { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Delay { delay_ms, feedback_percent, feedforward_percent };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn set_processing_node_limiter_params(
        &self,
        system_name: &str,
        ceiling_db: i32,
        floor_db: i32,
        symmetric: bool,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Limiter { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Limiter { ceiling_db, floor_db, symmetric };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn set_processing_node_hpf_params(
        &self,
        system_name: &str,
        freq_hz: i32,
        resonance_x10: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Hpf { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Hpf { freq_hz, resonance_x10 };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn set_processing_node_reverb_params(&self, system_name: &str, mix_percent: i32, bypassed: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Reverb { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Reverb { mix_percent };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn set_processing_node_widener_params(&self, system_name: &str, width_percent: i32, bypassed: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Widener { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Widener { width_percent };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn set_processing_node_pan_params(&self, system_name: &str, balance_percent: i32, bypassed: bool) -> Result<(), BackendError> {
        let mut graph = self.lock();
        let Some(node) = graph.processing_nodes.iter_mut().find(|node| node.system_name == system_name) else {
            return Err(BackendError::Message(format!("processing node not found: {system_name}")));
        };
        if let ProcessingNodeKind::Pan { .. } = &node.kind {
            node.kind = ProcessingNodeKind::Pan { balance_percent };
        }
        node.bypassed = bypassed;
        Ok(())
    }

    fn revert_to_plain_device(&self, _device: &Device, _wait_for_node: bool) -> Result<(), BackendError> {
        Ok(())
    }

    fn hold_sink_inputs_for_swap(&self, _device_system_name: &str) -> Result<Vec<String>, BackendError> {
        Ok(Vec::new())
    }

    fn release_held_sink_inputs(&self, _held_streams: &[String], _target_system_name: &str) -> Result<(), BackendError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `from_env()` is the only thing sensitive to `PIPE_DECK_MOCK_SCENARIO`
    /// — `new()` (used by ~100 other tests across the crate) never reads it
    /// — but serializes anyway, matching the crate's `PIPE_DECK_CONFIG_DIR`
    /// convention (`config::store::lock_config_dir_env`), so a second test
    /// added here later can't race this one.
    fn lock_mock_scenario_env() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn from_env_loads_a_scenario_file_when_set() {
        let _guard = lock_mock_scenario_env();
        let dir = std::env::temp_dir().join(format!("pipe-deck-mock-scenario-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let scenario_path = dir.join("test-scenario.yaml");
        std::fs::write(
            &scenario_path,
            r#"
version: 1
id: test
name: Test Scenario
devices:
  - id: sink-a
    label: Sink A
    kind: virtual
    direction: output
streams: []
routes: []
"#,
        )
        .unwrap();

        std::env::set_var("PIPE_DECK_MOCK_SCENARIO", &scenario_path);
        let backend = MockAudioBackend::from_env();
        std::env::remove_var("PIPE_DECK_MOCK_SCENARIO");
        let _ = std::fs::remove_dir_all(&dir);

        let graph = backend.fetch_graph().unwrap();
        assert_eq!(graph.devices.len(), 1);
        assert_eq!(graph.devices[0].id, "sink-a");
        assert!(graph.notice.unwrap().contains("Test Scenario"));
    }

    #[test]
    fn from_env_falls_back_to_sample_graph_when_unset() {
        let _guard = lock_mock_scenario_env();
        std::env::remove_var("PIPE_DECK_MOCK_SCENARIO");

        let backend = MockAudioBackend::from_env();
        let graph = backend.fetch_graph().unwrap();
        assert_eq!(graph.devices.len(), MockAudioBackend::sample_graph().devices.len());
    }

    #[test]
    fn from_env_falls_back_to_sample_graph_on_invalid_path() {
        let _guard = lock_mock_scenario_env();
        std::env::set_var("PIPE_DECK_MOCK_SCENARIO", "/nonexistent/pipe-deck-scenario.yaml");
        let backend = MockAudioBackend::from_env();
        std::env::remove_var("PIPE_DECK_MOCK_SCENARIO");

        let graph = backend.fetch_graph().unwrap();
        assert_eq!(graph.devices.len(), MockAudioBackend::sample_graph().devices.len());
    }

    #[test]
    fn play_sound_records_the_call() {
        let backend = MockAudioBackend::new();
        let dir = std::env::temp_dir().join(format!("pipe-deck-play-sound-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.wav");
        std::fs::write(&clip, b"not real audio, just needs to exist").unwrap();

        backend.play_sound(&clip, "pipe-deck-virtual-mic", 100).unwrap();
        backend.play_sound(&clip, "pipe-deck-virtual-mic", 40).unwrap();

        let _ = std::fs::remove_dir_all(&dir);

        let played = backend.played_sounds.lock().unwrap();
        assert_eq!(played.len(), 2);
        assert_eq!(played[0], (clip.clone(), "pipe-deck-virtual-mic".to_string(), 100));
        assert_eq!(played[1], (clip, "pipe-deck-virtual-mic".to_string(), 40));
    }

    #[test]
    fn stop_sound_records_the_call() {
        let backend = MockAudioBackend::new();
        backend.stop_sound().unwrap();
        backend.stop_sound().unwrap();

        assert_eq!(*backend.soundboard_stop_calls.lock().unwrap(), 2);
    }
}
