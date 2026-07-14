use super::{AudioBackend, BackendError, GraphListener};
use crate::core::models::{Device, MixSourceSpec, RuntimeGraph};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use std::collections::HashSet;

/// Proof-of-concept second backend: every method fails, but it's a
/// structurally independent `AudioBackend` implementation that compiles and
/// can be wired into `create_backend()` without touching any engine call
/// site — demonstrating the boundary issue #68 exists to guarantee ahead of
/// a real macOS/Windows backend (#69/#70).
pub struct StubBackend;

impl StubBackend {
    pub fn new() -> Self {
        Self
    }
}

impl AudioBackend for StubBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn subscribe(&self, _listener: GraphListener) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn set_device_volume(&self, _graph: &RuntimeGraph, _device_id: &str, _percent: u8) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn set_device_mute(&self, _graph: &RuntimeGraph, _device_id: &str, _muted: bool) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn set_stream_volume(&self, _graph: &RuntimeGraph, _stream_id: &str, _percent: u8) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn set_stream_mute(&self, _graph: &RuntimeGraph, _stream_id: &str, _muted: bool) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn clear_stream_target(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn route_stream(&self, _graph: &RuntimeGraph, _stream_id: &str, _target_device_id: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn route_device(&self, _graph: &RuntimeGraph, _source_device_id: &str, _target_device_ids: &[String]) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn sync_live_routing_graph(&self, _graph: &mut RuntimeGraph) {}

    fn apply_user_cleared_routes(
        &self,
        _graph: &mut RuntimeGraph,
        _cleared_streams: &HashSet<StreamIdentityKey>,
        _cleared_devices: &HashSet<String>,
    ) {
    }

    fn apply_graph_routing(&self, _graph: &mut RuntimeGraph, _ctx: &ApplyRulesContext<'_>) {}

    fn apply_virtual_mic_mix(&self, _virtual_input: &Device, _mix_sources: &[MixSourceSpec]) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn set_mix_source_volume(&self, _virtual_input_system_name: &str, _source_system_name: &str, _percent: u8) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn set_mix_source_mute(&self, _virtual_input_system_name: &str, _source_system_name: &str, _muted: bool) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "no audio backend implemented for this platform yet".into(),
        ))
    }

    fn apply_device_aliases_and_levels(&self, _devices: &mut [Device]) {}
}
