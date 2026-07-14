use crate::core::models::{
    Device, DeviceDirection, DeviceKind, Link, MixSourceSpec, RuntimeGraph, Stream, StreamDirection,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use crate::backend::{BackendError, GraphListener, AudioBackend};
use std::collections::HashSet;

/// Static sample graph for development until real PipeWire enumeration lands.
/// Data is stable — no simulated changes or background polling.
pub struct MockAudioBackend;

impl MockAudioBackend {
    pub fn new() -> Self {
        Self
    }

    fn sample_graph() -> RuntimeGraph {
        RuntimeGraph {
            devices: vec![
                mock_device("sink-chat", "Chat", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("sink-music", "Music", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("sink-game", "Game", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("sink-browser", "Browser", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("sink-stream-mix", "Stream Mix", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("sink-headphones", "Headphones", DeviceKind::Physical, DeviceDirection::Output),
                mock_device("sink-speakers", "Speakers", DeviceKind::Physical, DeviceDirection::Output),
                mock_device("sink-stream-output", "Stream Output", DeviceKind::Virtual, DeviceDirection::Output),
                mock_device("source-mic", "Microphone", DeviceKind::Physical, DeviceDirection::Input),
                mock_device("source-mic-filtered", "Mic (Filtered)", DeviceKind::Virtual, DeviceDirection::Input),
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
                    current_targets: Vec::new(),
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
                    current_targets: Vec::new(),
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
                    current_targets: Vec::new(),
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
                    current_targets: Vec::new(),
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
                    current_targets: Vec::new(),
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
                // Capture path
                Link {
                    id: "link-obs-mic".into(),
                    source_id: "stream-obs".into(),
                    target_id: "source-mic-filtered".into(),
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

impl AudioBackend for MockAudioBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        Ok(Self::sample_graph())
    }

    fn subscribe(&self, _listener: GraphListener) -> Result<(), BackendError> {
        // Mock adapter is static; real PipeWire adapter will push live updates here.
        Ok(())
    }

    // Mutation for the mock data source happens in `CoreEngine`'s own
    // `data_source == "mock"` branches today (see `core/engine/mock.rs`),
    // which short-circuit before ever calling the adapter — these are
    // unreachable no-ops until that mock state is consolidated onto this
    // backend (tracked as a later step of issue #68).
    fn set_device_volume(&self, _graph: &RuntimeGraph, _device_id: &str, _percent: u8) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_device_mute(&self, _graph: &RuntimeGraph, _device_id: &str, _muted: bool) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_stream_volume(&self, _graph: &RuntimeGraph, _stream_id: &str, _percent: u8) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_stream_mute(&self, _graph: &RuntimeGraph, _stream_id: &str, _muted: bool) -> Result<(), BackendError> {
        Ok(())
    }

    fn clear_stream_target(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        Ok(())
    }

    // The mock sample graph has no real pactl/pw-link session behind it, so
    // reconciliation that requires live PipeWire queries is a deliberate
    // no-op rather than shelling out to system tools with nothing meaningful
    // to report. `apply_user_cleared_routes` and the alias half of
    // `apply_device_aliases_and_levels` are pure in-memory graph/config
    // operations with no such dependency, so they still run for real —
    // `routing_ops::clear_stream_target`'s mock path and device aliasing
    // both rely on that actually happening.
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

    fn apply_virtual_mic_mix(&self, _virtual_input: &Device, _mix_sources: &[MixSourceSpec]) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_mix_source_volume(&self, _virtual_input_system_name: &str, _source_system_name: &str, _percent: u8) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_mix_source_mute(&self, _virtual_input_system_name: &str, _source_system_name: &str, _muted: bool) -> Result<(), BackendError> {
        Ok(())
    }

    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]) {
        crate::backend::linux::graph_enrich::apply_device_aliases(devices);
    }
}
