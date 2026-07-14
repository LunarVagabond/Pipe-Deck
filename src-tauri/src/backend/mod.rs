pub mod linux;
pub mod mock;
pub mod stub;

use crate::core::models::{Device, MixSourceSpec, RuntimeGraph};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("{0}")]
    Message(String),
}

pub type GraphListener = Box<dyn Fn(RuntimeGraph) + Send + Sync>;

pub trait AudioBackend: Send + Sync {
    // Graph fetch/subscribe.
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError>;
    fn subscribe(&self, listener: GraphListener) -> Result<(), BackendError>;

    // Volume / mute. `graph` is passed alongside the domain id because
    // resolving an id to whatever the backend addresses volume/mute by
    // (a pactl sink-input index, a Core Audio device UID, ...) needs the
    // already-fetched graph, not a second live lookup.
    fn set_device_volume(&self, graph: &RuntimeGraph, device_id: &str, percent: u8) -> Result<(), BackendError>;
    fn set_device_mute(&self, graph: &RuntimeGraph, device_id: &str, muted: bool) -> Result<(), BackendError>;
    fn set_stream_volume(&self, graph: &RuntimeGraph, stream_id: &str, percent: u8) -> Result<(), BackendError>;
    fn set_stream_mute(&self, graph: &RuntimeGraph, stream_id: &str, muted: bool) -> Result<(), BackendError>;

    // Routing: clearing a stream's route back to default. `route_stream`/
    // `route_device` (create/change a route) stay engine-side today,
    // reached via `graph_routing`/`graph_sync` reconciliation rather than a
    // single-link trait call — see docs/Decisions.md PD-019 and issue #68
    // for why that reconciliation logic isn't collapsed into this trait yet.
    fn clear_stream_target(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError>;

    // Graph/routing reconciliation. These stay call-granularity-agnostic on
    // purpose (see PD-019 and issue #68): the Linux impl internally discovers
    // and reconciles live pw-link/pactl state in one batched pass rather than
    // one link at a time, and a future backend is free to do the same in
    // whatever shape its platform's routing APIs need — the trait boundary is
    // "engine code doesn't name `backend::linux` directly", not "every route
    // change is one trait call."
    fn sync_live_routing_graph(&self, graph: &mut RuntimeGraph);
    fn apply_user_cleared_routes(
        &self,
        graph: &mut RuntimeGraph,
        cleared_streams: &HashSet<StreamIdentityKey>,
        cleared_devices: &HashSet<String>,
    );
    fn apply_graph_routing(&self, graph: &mut RuntimeGraph, ctx: &ApplyRulesContext<'_>);

    // Virtual device mix sources / aliases / levels. Virtual device
    // create/remove itself stays engine-held via `VirtualDeviceRegistry`
    // (see core/restore.rs, core/engine/virtual_ops.rs) rather than moving
    // behind this trait — the registry doesn't just track bookkeeping, it
    // *is* the Linux creation mechanism, so splitting "system-level create"
    // from "registry state" isn't a clean boundary without a deeper redesign
    // than #68 calls for.
    fn apply_virtual_mic_mix(&self, virtual_input: &Device, mix_sources: &[MixSourceSpec]) -> Result<(), BackendError>;
    fn set_mix_source_volume(&self, virtual_input_system_name: &str, source_system_name: &str, percent: u8) -> Result<(), BackendError>;
    fn set_mix_source_mute(&self, virtual_input_system_name: &str, source_system_name: &str, muted: bool) -> Result<(), BackendError>;
    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]);
}

/// Backend selection is compile-time/explicit-factory only (PD-019) — never
/// a runtime plugin.
pub fn create_backend() -> Box<dyn AudioBackend> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Box::new(mock::MockAudioBackend::new());
    }

    #[cfg(target_os = "linux")]
    {
        match linux::LinuxPipeWireBackend::new() {
            Ok(backend) => Box::new(backend),
            Err(error) => {
                eprintln!("PipeWire enumeration unavailable: {error}");
                Box::new(EmptyAudioBackend {
                    notice: format!("PipeWire unavailable: {error}"),
                })
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Real macOS/Windows backends land as #69/#70; `StubBackend` only
        // proves the trait boundary holds on a second platform target.
        Box::new(stub::StubBackend::new())
    }
}

struct EmptyAudioBackend {
    notice: String,
}

impl AudioBackend for EmptyAudioBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        Ok(RuntimeGraph {
            notice: Some(self.notice.clone()),
            ..RuntimeGraph::default()
        })
    }

    fn subscribe(&self, _listener: GraphListener) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_device_volume(&self, _graph: &RuntimeGraph, _device_id: &str, _percent: u8) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_device_mute(&self, _graph: &RuntimeGraph, _device_id: &str, _muted: bool) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_stream_volume(&self, _graph: &RuntimeGraph, _stream_id: &str, _percent: u8) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_stream_mute(&self, _graph: &RuntimeGraph, _stream_id: &str, _muted: bool) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn clear_stream_target(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
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
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_mix_source_volume(&self, _virtual_input_system_name: &str, _source_system_name: &str, _percent: u8) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_mix_source_mute(&self, _virtual_input_system_name: &str, _source_system_name: &str, _muted: bool) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn apply_device_aliases_and_levels(&self, _devices: &mut [Device]) {}
}
