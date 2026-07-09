use crate::core::models::{
    Device, DeviceDirection, DeviceKind, Link, RuntimeGraph, Stream, StreamDirection,
};
use crate::pipewire::adapter::{AdapterError, GraphListener, PipeWireAdapter};

/// Static sample graph for development until real PipeWire enumeration lands.
/// Data is stable — no simulated changes or background polling.
pub struct MockPipeWireAdapter;

impl MockPipeWireAdapter {
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
        volume_percent: Some(70),
        muted: Some(false),
        current_target: None,
    }
}

impl PipeWireAdapter for MockPipeWireAdapter {
    fn fetch_graph(&self) -> Result<RuntimeGraph, AdapterError> {
        Ok(Self::sample_graph())
    }

    fn subscribe(&self, _listener: GraphListener) -> Result<(), AdapterError> {
        // Mock adapter is static; real PipeWire adapter will push live updates here.
        Ok(())
    }
}
