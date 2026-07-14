use super::{AudioBackend, BackendError, GraphListener};
use crate::core::models::RuntimeGraph;

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
}
