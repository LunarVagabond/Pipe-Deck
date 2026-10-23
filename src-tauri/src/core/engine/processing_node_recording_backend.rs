use crate::backend::{AudioBackend, BackendError, GraphListener};
use crate::core::models::{
    Device, DeviceDirection, EffectChainConfig, LatencyPathNode, LatencyPingResult, MixSourceSpec,
    PortDirection, ProcessingNode, RuntimeGraph, VirtualDeviceInfo, VirtualDeviceResult,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BackendCall {
    Other(&'static str),
    Loaded(String),
    Relink {
        node: ProcessingNode,
        port_index: u32,
        direction: PortDirection,
        peer_id: Option<String>,
    },
}

pub(super) struct RecordingBackend {
    pub calls: Arc<Mutex<Vec<BackendCall>>>,
    pub graph: Arc<Mutex<RuntimeGraph>>,
    pub loaded: Arc<AtomicBool>,
    pub fail_relink: Arc<AtomicBool>,
}

// Record all trait entries, including otherwise-unused default methods.
macro_rules! record_calls {
    ($(fn $name:ident(&self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?;)*) => {
        $(fn $name(&self $(, $arg: $ty)*) $(-> $ret)? {
            self.calls.lock().unwrap().push(BackendCall::Other(stringify!($name)));
            crate::backend::stub::StubBackend.$name($($arg),*)
        })*
    };
}

impl AudioBackend for RecordingBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        self.calls
            .lock()
            .unwrap()
            .push(BackendCall::Other("fetch_graph"));
        Ok(self.graph.lock().unwrap().clone())
    }

    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo> {
        self.calls
            .lock()
            .unwrap()
            .push(BackendCall::Other("list_virtual_devices"));
        Vec::new()
    }

    fn is_processing_node_loaded(&self, system_name: &str) -> bool {
        self.calls
            .lock()
            .unwrap()
            .push(BackendCall::Loaded(system_name.into()));
        self.loaded.load(Ordering::SeqCst)
    }

    fn relink_processing_node_port(
        &self,
        graph: &RuntimeGraph,
        system_name: &str,
        port_index: u32,
        direction: PortDirection,
        peer_id: Option<&str>,
    ) -> Result<(), BackendError> {
        let node = graph
            .processing_nodes
            .iter()
            .find(|node| node.system_name == system_name)
            .unwrap()
            .clone();
        self.calls.lock().unwrap().push(BackendCall::Relink {
            node,
            port_index,
            direction,
            peer_id: peer_id.map(str::to_owned),
        });
        if self.fail_relink.load(Ordering::SeqCst) {
            Err(BackendError::Message("simulated relink failure".into()))
        } else {
            Ok(())
        }
    }

    record_calls! {
        fn subscribe(&self, listener: GraphListener) -> Result<(), BackendError>;
        fn set_device_volume(
            &self,
            graph: &RuntimeGraph,
            device_id: &str,
            percent: u8,
        ) -> Result<(), BackendError>;
        fn set_device_mute(
            &self,
            graph: &RuntimeGraph,
            device_id: &str,
            muted: bool,
        ) -> Result<(), BackendError>;
        fn set_stream_volume(
            &self,
            graph: &RuntimeGraph,
            stream_id: &str,
            percent: u8,
        ) -> Result<(), BackendError>;
        fn set_stream_mute(
            &self,
            graph: &RuntimeGraph,
            stream_id: &str,
            muted: bool,
        ) -> Result<(), BackendError>;
        fn default_output_device_name(&self) -> Option<String>;
        fn set_default_output_device(&self, _system_name: &str) -> Result<(), BackendError>;
        fn clear_stream_target(
            &self,
            graph: &RuntimeGraph,
            stream_id: &str,
            previous_target_device_id: Option<&str>,
        ) -> Result<(), BackendError>;
        fn route_stream(
            &self,
            graph: &RuntimeGraph,
            stream_id: &str,
            target_device_id: &str,
        ) -> Result<(), BackendError>;
        fn sync_live_routing_graph(&self, graph: &mut RuntimeGraph);
        fn apply_user_cleared_routes(
            &self,
            graph: &mut RuntimeGraph,
            cleared_streams: &HashSet<StreamIdentityKey>,
            cleared_devices: &HashSet<String>,
        );
        fn apply_graph_routing(&self, graph: &mut RuntimeGraph, ctx: &ApplyRulesContext<'_>);
        fn apply_virtual_mic_mix(
            &self,
            virtual_input: &Device,
            mix_sources: &[MixSourceSpec],
        ) -> Result<(), BackendError>;
        fn set_mix_source_volume(
            &self,
            virtual_input_system_name: &str,
            source_system_name: &str,
            percent: u8,
        ) -> Result<(), BackendError>;
        fn set_mix_source_mute(
            &self,
            virtual_input_system_name: &str,
            source_system_name: &str,
            muted: bool,
        ) -> Result<(), BackendError>;
        fn disconnect_all_virtual_mic_mixes(
            &self,
            virtual_input_system_name: &str,
        ) -> Result<(), BackendError>;
        fn apply_device_aliases_and_levels(&self, devices: &mut [Device]);
        fn create_virtual_output(
            &self,
            label: &str,
            multi: bool,
        ) -> Result<VirtualDeviceResult, BackendError>;
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
        fn set_virtual_device_alias(&self, system_name: &str, alias: &str) -> Result<(), BackendError>;
        fn play_sound(
            &self,
            path: &std::path::Path,
            target_system_name: &str,
            volume_percent: u8,
        ) -> Result<(), BackendError>;
        fn stop_sound(&self) -> Result<(), BackendError>;
        fn find_live_node_id(&self, node_name: &str) -> Result<Option<u32>, BackendError>;
        fn monitor_routes_for_source(&self, _source_system_name: &str) -> Vec<String>;
        fn is_routed_to(
            &self,
            _source_system_name: &str,
            _target_system_name: &str,
            _target_is_input: bool,
        ) -> bool;
        fn device_is_live(&self, _system_name: &str, _direction: DeviceDirection) -> bool;
        fn platform_audio_version(&self) -> Option<String>;
        fn measure_latency_ping(
            &self,
            _path: &[LatencyPathNode],
        ) -> Result<LatencyPingResult, BackendError>;
        fn effect_capabilities(&self) -> crate::pipewire::fx_capability::FxCapabilities;
        fn preflight_effect_chain(
            &self,
            config: &EffectChainConfig,
        ) -> crate::pipewire::fx_validate::PreflightResult;
        fn push_effect_chain_live_params(
            &self,
            _device_system_name: &str,
            node_id: u32,
            config: &EffectChainConfig,
        ) -> Result<(), BackendError>;
        fn revert_to_plain_device(
            &self,
            device: &Device,
            wait_for_node: bool,
        ) -> Result<(), BackendError>;
        fn hold_sink_inputs_for_swap(
            &self,
            device_system_name: &str,
        ) -> Result<Vec<String>, BackendError>;
        fn release_held_sink_inputs(
            &self,
            held_streams: &[String],
            target_system_name: &str,
        ) -> Result<(), BackendError>;
        fn list_mic_feeds(
            &self,
            target_system_name: &str,
            target_is_virtual_source: bool,
        ) -> Vec<String>;
        fn relink_mic_feeds(
            &self,
            feeders: &[String],
            from_system_name: &str,
            to_system_name: &str,
            to_is_virtual_source: bool,
        ) -> Result<(), BackendError>;
        fn load_effect_chain(
            &self,
            _device: &Device,
            _config: &EffectChainConfig,
            _downstream_targets: &[Device],
            _mic_feeders: &[String],
        ) -> Result<String, BackendError>;
        fn unload_effect_chain(&self, _device_system_name: &str) -> Result<(), BackendError>;
        fn is_effect_chain_loaded(&self, _device_system_name: &str) -> bool;
        fn set_effect_chain_live_params(
            &self,
            _device_system_name: &str,
            _config: &EffectChainConfig,
        ) -> Result<(), BackendError>;
        fn load_processing_node(&self, _node: &ProcessingNode) -> Result<(), BackendError>;
        fn unload_processing_node(&self, _system_name: &str) -> Result<(), BackendError>;
        fn set_processing_node_input_gain(
            &self,
            _system_name: &str,
            _peer_system_name: &str,
            _gain_percent: u8,
            _muted: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_volume(
            &self,
            _system_name: &str,
            _volume_percent: u8,
            _muted: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_eq_params(
            &self,
            _system_name: &str,
            _eq_sub: i32,
            _eq_bass: i32,
            _eq_mid: i32,
            _eq_treble: i32,
            _eq_air: i32,
            _output_gain: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_delay_params(
            &self,
            _system_name: &str,
            _delay_ms: i32,
            _feedback_percent: i32,
            _feedforward_percent: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_limiter_params(
            &self,
            _system_name: &str,
            _ceiling_db: i32,
            _floor_db: i32,
            _symmetric: bool,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_hpf_params(
            &self,
            _system_name: &str,
            _freq_hz: i32,
            _resonance_x10: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_reverb_params(
            &self,
            _system_name: &str,
            _mix_percent: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_widener_params(
            &self,
            _system_name: &str,
            _width_percent: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_pan_params(
            &self,
            _system_name: &str,
            _balance_percent: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
        fn set_processing_node_compressor_params(
            &self,
            _system_name: &str,
            _threshold_db: i32,
            _ratio_x10: i32,
            _attack_ms: i32,
            _release_ms: i32,
            _makeup_gain_db: i32,
            _bypassed: bool,
        ) -> Result<(), BackendError>;
    }
}
