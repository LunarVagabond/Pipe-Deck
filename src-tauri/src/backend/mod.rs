pub mod linux;
pub mod mock;
pub mod stub;

use crate::core::models::{Device, DeviceDirection, MixSourceSpec, RuntimeGraph, VirtualDeviceInfo, VirtualDeviceResult};
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

/// Shared by every backend's virtual-device system_name derivation — moved
/// here (from `backend::linux::virtual_devices`, still re-exported there)
/// so `MockAudioBackend` doesn't need to depend on `backend::linux`.
pub fn slugify(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "device".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod slugify_tests {
    use super::slugify;

    #[test]
    fn slugifies_names_with_punctuation_and_case() {
        assert_eq!(slugify("Game Mix"), "game-mix");
        assert_eq!(slugify("My Mic!!!"), "my-mic");
    }

    #[test]
    fn empty_or_all_punctuation_falls_back_to_device() {
        assert_eq!(slugify(""), "device");
        assert_eq!(slugify("!!!"), "device");
    }
}

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

    // Routing: set or clear a single stream/device route.
    fn clear_stream_target(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError>;
    fn route_stream(&self, graph: &RuntimeGraph, stream_id: &str, target_device_id: &str) -> Result<(), BackendError>;
    fn route_device(&self, graph: &RuntimeGraph, source_device_id: &str, target_device_ids: &[String]) -> Result<(), BackendError>;

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

    // Virtual device mix sources / aliases / levels.
    fn apply_virtual_mic_mix(&self, virtual_input: &Device, mix_sources: &[MixSourceSpec]) -> Result<(), BackendError>;
    fn set_mix_source_volume(&self, virtual_input_system_name: &str, source_system_name: &str, percent: u8) -> Result<(), BackendError>;
    fn set_mix_source_mute(&self, virtual_input_system_name: &str, source_system_name: &str, muted: bool) -> Result<(), BackendError>;
    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]);

    // Virtual device lifecycle. `create_virtual_output`/`create_virtual_input`
    // are for user-initiated new devices, where system_name is derived from
    // the label. `restore_virtual_device` is for config-driven recreation
    // (core/restore.rs) where system_name is already fixed (the persisted
    // slug) and must NOT be re-derived from a possibly-since-renamed label.
    fn create_virtual_output(&self, label: &str, multi: bool) -> Result<VirtualDeviceResult, BackendError>;
    fn create_virtual_input(&self, label: &str) -> Result<VirtualDeviceResult, BackendError>;
    fn restore_virtual_device(
        &self,
        system_name: &str,
        label: &str,
        direction: DeviceDirection,
        multi: bool,
        mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError>;
    fn remove_virtual_device(&self, system_name: &str) -> Result<(), BackendError>;
    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo>;
    fn set_virtual_device_alias(&self, system_name: &str, alias: &str) -> Result<(), BackendError>;

    // Live routing-state queries used only as rule-matching fallbacks when
    // `RuntimeGraph`'s own `current_targets`/`current_target` are stale or
    // missing (see core/rules/matching.rs, core/rules/evaluation.rs). A
    // graph-derived answer is always tried first by the caller; these exist
    // for the rare case a live re-check is genuinely needed.
    fn monitor_routes_for_source(&self, _source_system_name: &str) -> Vec<String> {
        Vec::new()
    }

    fn is_routed_to(&self, _source_system_name: &str, _target_system_name: &str, _target_is_input: bool) -> bool {
        false
    }

    // Backing audio-stack version, for display only (Settings/about footer).
    // `None` means "unknown/unavailable" rather than an error — every backend
    // gets this for free unless it overrides it.
    fn platform_audio_version(&self) -> Option<String> {
        None
    }
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

    fn route_stream(&self, _graph: &RuntimeGraph, _stream_id: &str, _target_device_id: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn route_device(&self, _graph: &RuntimeGraph, _source_device_id: &str, _target_device_ids: &[String]) -> Result<(), BackendError> {
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

    fn monitor_routes_for_source(&self, _source_system_name: &str) -> Vec<String> {
        Vec::new()
    }

    fn is_routed_to(&self, _source_system_name: &str, _target_system_name: &str, _target_is_input: bool) -> bool {
        false
    }

    fn create_virtual_output(&self, _label: &str, _multi: bool) -> Result<VirtualDeviceResult, BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn create_virtual_input(&self, _label: &str) -> Result<VirtualDeviceResult, BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn restore_virtual_device(
        &self,
        _system_name: &str,
        _label: &str,
        _direction: DeviceDirection,
        _multi: bool,
        _mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn remove_virtual_device(&self, _system_name: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo> {
        Vec::new()
    }

    fn set_virtual_device_alias(&self, _system_name: &str, _alias: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }
}
