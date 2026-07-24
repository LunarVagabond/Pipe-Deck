use crate::backend::slugify;
use crate::config::ConfigStore;
use crate::core::models::{
    ApplyResult, PortDirection, ProcessingNode, ProcessingNodeKind, ProcessingNodePort,
    ProcessingNodeSpec, ProcessingNodeSpecKind, RuntimeGraph,
};
use chrono::Utc;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn list_processing_nodes(&self) -> &[ProcessingNode] {
        &self.graph.processing_nodes
    }

    /// Wires `peer_id` (a device or stream id) into `node_id`'s next free
    /// port on the given `direction`, or a freshly appended one if every
    /// existing port of that direction is already occupied — this is how a
    /// Mixer's inputs or a Fan-out's outputs grow with each connection.
    /// Single-input kinds (Fan-out/EQ/stub all take exactly one signal in)
    /// reject a second input connection instead of silently accepting one
    /// nothing downstream expects.
    pub fn connect_processing_node_port(
        &mut self,
        node_id: &str,
        direction: PortDirection,
        peer_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;

        let peer_system_name = resolve_system_name_for_id(&self.graph, peer_id)
            .ok_or_else(|| EngineError::InvalidInput(format!("peer not found: {peer_id}")))?;

        let ports = match direction {
            PortDirection::Input => &node.inputs,
            PortDirection::Output => &node.outputs,
        };
        if direction == PortDirection::Input
            && !matches!(node.kind, ProcessingNodeKind::Mixer { .. })
            && ports.iter().any(|port| port.connected_id.is_some())
        {
            return Err(EngineError::InvalidInput(format!(
                "{node_id} accepts only one input - disconnect the existing one first"
            )));
        }
        let port_index = ports
            .iter()
            .find(|port| port.connected_id.is_none())
            .map(|port| port.index)
            .unwrap_or(ports.len() as u32);

        self.adapter
            .relink_processing_node_port(&self.graph, &node.system_name, port_index, direction, Some(peer_id))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .upsert_processing_node_port(node_id, direction, port_index, &peer_system_name)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }

    pub fn disconnect_processing_node_port(
        &mut self,
        node_id: &str,
        direction: PortDirection,
        port_index: u32,
    ) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;

        self.adapter
            .relink_processing_node_port(&self.graph, &node.system_name, port_index, direction, None)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .remove_processing_node_port(node_id, direction, port_index)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }

    /// Creates a Mixer/Fan-out/EQ/stub processing node (PD-032). Freshly
    /// created with no ports wired — connecting it into the graph is a
    /// separate, later step (issue #293 phases 2-5), not part of creation.
    pub fn create_processing_node(
        &mut self,
        label: &str,
        kind: ProcessingNodeSpecKind,
    ) -> Result<ProcessingNode, EngineError> {
        let slug = slugify(label);
        let kind_slug = spec_kind_slug(&kind);
        let id = format!("processing-{kind_slug}-{slug}");
        if self.graph.processing_nodes.iter().any(|node| node.id == id) {
            return Err(EngineError::InvalidInput(format!(
                "a processing node with this name already exists: {id}"
            )));
        }

        let spec = ProcessingNodeSpec {
            id: id.clone(),
            slug,
            label: label.to_string(),
            created_at: Utc::now().to_rfc3339(),
            kind,
            input_sources: Vec::new(),
            output_targets: Vec::new(),
        };

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .add_processing_node(spec.clone())
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        let node = processing_node_from_spec(&spec, &self.graph);
        self.adapter
            .load_processing_node(&node)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        self.refresh_graph()?;
        self.graph
            .processing_nodes
            .iter()
            .find(|node| node.id == id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found after create: {id}")))
    }

    /// Removes a processing node. Rejects removal outright (rather than
    /// guessing a many-to-many relink) when more than one input or more than
    /// one output is still connected — see PD-032's "ambiguous relink" rule,
    /// the direct lesson from #105's incomplete-teardown failure mode.
    pub fn remove_processing_node(&mut self, id: &str) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {id}")))?;

        let connected_inputs = node.inputs.iter().filter(|port| port.connected_id.is_some()).count();
        let connected_outputs = node.outputs.iter().filter(|port| port.connected_id.is_some()).count();
        if connected_inputs > 1 || connected_outputs > 1 {
            return Err(EngineError::InvalidInput(format!(
                "cannot remove {id}: relinking {connected_inputs} input(s) and {connected_outputs} output(s) would be ambiguous - disconnect down to at most one side first"
            )));
        }

        self.adapter
            .unload_processing_node(&node.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .remove_processing_node(id)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }
}

/// Backfills `graph.processing_nodes` from persisted config, the same role
/// `merge_virtual_devices` plays for `graph.devices` — necessary because a
/// real backend's `fetch_graph()` (built on `pw_dump`, which skips every
/// `pipe-deck-*`-named node outright) never includes processing nodes on its
/// own. Skipped for the mock backend: `MockAudioBackend::fetch_graph()`
/// already returns its own internal `processing_nodes` state directly (see
/// `load_processing_node`/`unload_processing_node`'s mock implementation),
/// so re-deriving from `ConfigStore` here would be a second, divergent
/// source of truth for the same data.
pub(super) fn merge_processing_nodes(graph: &mut RuntimeGraph, adapter: &dyn crate::backend::AudioBackend) {
    if graph.data_source == "mock" {
        return;
    }

    let specs = ConfigStore::new().processing_nodes();
    let nodes: Vec<ProcessingNode> = specs
        .iter()
        .map(|spec| {
            let mut node = processing_node_from_spec(spec, graph);
            node.live = adapter.is_processing_node_loaded(&node.system_name);
            node
        })
        .collect();
    graph.processing_nodes = nodes;
}

fn spec_kind_slug(kind: &ProcessingNodeSpecKind) -> &'static str {
    match kind {
        ProcessingNodeSpecKind::Mixer => "mixer",
        ProcessingNodeSpecKind::FanOut => "fan_out",
        ProcessingNodeSpecKind::Eq5Band { .. } => "eq5band",
        ProcessingNodeSpecKind::Stub { .. } => "stub",
    }
}

fn resolve_system_name_for_id(graph: &RuntimeGraph, id: &str) -> Option<String> {
    graph
        .devices
        .iter()
        .find(|device| device.id == id)
        .map(|device| device.system_name.clone())
        .or_else(|| {
            graph
                .streams
                .iter()
                .find(|stream| stream.id == id)
                .and_then(|stream| stream.system_name.clone())
        })
}

fn resolve_id_for_system_name(graph: &RuntimeGraph, system_name: &str) -> Option<String> {
    graph
        .devices
        .iter()
        .find(|device| device.system_name == system_name)
        .map(|device| device.id.clone())
        .or_else(|| {
            graph
                .streams
                .iter()
                .find(|stream| stream.system_name.as_deref() == Some(system_name))
                .map(|stream| stream.id.clone())
        })
}

/// Converts a persisted `ProcessingNodeSpec` into its runtime `ProcessingNode`
/// — system_name derivation (`pipe-deck-proc-{kind}-{slug}`, PD-032) and
/// kind-specific field mapping both live here so `merge_processing_nodes`
/// (real-backend refresh path) and `create_processing_node` share one
/// conversion instead of two independently-drifting ones. `graph` resolves
/// each port's persisted `source_system_name`/target to the device/stream id
/// currently wearing that system name (may be `None` if nothing live answers
/// to it right now — same "unresolved is not an error" reasoning as
/// `Device.current_target`).
pub(super) fn processing_node_from_spec(spec: &ProcessingNodeSpec, graph: &RuntimeGraph) -> ProcessingNode {
    let kind_slug = spec_kind_slug(&spec.kind);
    let system_name = format!("pipe-deck-proc-{kind_slug}-{}", spec.slug);

    let kind = match &spec.kind {
        ProcessingNodeSpecKind::Mixer => ProcessingNodeKind::Mixer {
            input_gains_percent: spec.input_sources.iter().map(|port| port.gain_percent).collect(),
        },
        ProcessingNodeSpecKind::FanOut => ProcessingNodeKind::FanOut,
        ProcessingNodeSpecKind::Eq5Band {
            eq_sub,
            eq_bass,
            eq_mid,
            eq_treble,
            eq_air,
            output_gain,
        } => ProcessingNodeKind::Eq5Band {
            eq_sub: *eq_sub,
            eq_bass: *eq_bass,
            eq_mid: *eq_mid,
            eq_treble: *eq_treble,
            eq_air: *eq_air,
            output_gain: *output_gain,
        },
        ProcessingNodeSpecKind::Stub { stub_kind } => ProcessingNodeKind::Stub { stub_kind: *stub_kind },
    };

    let inputs = spec
        .input_sources
        .iter()
        .enumerate()
        .map(|(index, port)| ProcessingNodePort {
            index: index as u32,
            connected_id: resolve_id_for_system_name(graph, &port.source_system_name),
        })
        .collect();
    let outputs = spec
        .output_targets
        .iter()
        .enumerate()
        .map(|(index, target)| ProcessingNodePort {
            index: index as u32,
            connected_id: resolve_id_for_system_name(graph, target),
        })
        .collect();

    ProcessingNode {
        id: spec.id.clone(),
        label: spec.label.clone(),
        kind,
        system_name,
        bypassed: false,
        live: false,
        inputs,
        outputs,
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d on purpose (mirrors `effects_ops::live_tests`): these hit
    //! a *real* PipeWire session via `pactl`/`pw-link`, unlike every other
    //! test in this crate. Only run via
    //! `cargo test --lib -- --ignored fan_out_node_round_trips_on_a_real_pipewire_session`.
    //! Creates disposable `pipe-deck-*`/`pipe-deck-proc-*` devices it removes
    //! itself; never touches anything the user configured.
    use super::*;
    use crate::backend::linux::{pactl, pw_link};

    #[test]
    #[ignore]
    fn fan_out_node_round_trips_on_a_real_pipewire_session() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let output_a = engine.create_virtual_output("Pipe Deck Live Fan A").expect("create target a");
        let output_b = engine.create_virtual_output("Pipe Deck Live Fan B").expect("create target b");

        let cleanup = |engine: &mut CoreEngine, node_id: Option<&str>| {
            if let Some(id) = node_id {
                let _ = engine.remove_processing_node(id);
            }
            let _ = engine.remove_virtual_device(&output_a.system_name);
            let _ = engine.remove_virtual_device(&output_b.system_name);
        };

        let node = match engine.create_processing_node("Pipe Deck Live Fan-out", ProcessingNodeSpecKind::FanOut) {
            Ok(node) => node,
            Err(error) => {
                cleanup(&mut engine, None);
                panic!("create_processing_node failed: {error}");
            }
        };

        if !pactl::sink_exists(&node.system_name).unwrap_or(false) {
            cleanup(&mut engine, Some(&node.id));
            panic!("fan-out sink did not appear after create_processing_node");
        }

        if let Err(error) = engine.connect_processing_node_port(&node.id, PortDirection::Output, &output_a.device_id) {
            cleanup(&mut engine, Some(&node.id));
            panic!("connect output a failed: {error}");
        }
        if let Err(error) = engine.connect_processing_node_port(&node.id, PortDirection::Output, &output_b.device_id) {
            cleanup(&mut engine, Some(&node.id));
            panic!("connect output b failed: {error}");
        }

        let linked_a = pw_link::is_sink_monitor_routed_to(&node.system_name, &output_a.system_name, false);
        let linked_b = pw_link::is_sink_monitor_routed_to(&node.system_name, &output_b.system_name, false);

        // Disconnecting output a must tear down only that leg, not b's —
        // the #105 lesson (incomplete/incorrect teardown) applied to a real
        // session rather than the mock's in-memory bookkeeping.
        let disconnect_result = engine.disconnect_processing_node_port(&node.id, PortDirection::Output, 0);
        let still_linked_b_after_disconnect =
            pw_link::is_sink_monitor_routed_to(&node.system_name, &output_b.system_name, false);
        let unlinked_a_after_disconnect =
            !pw_link::is_sink_monitor_routed_to(&node.system_name, &output_a.system_name, false);

        cleanup(&mut engine, Some(&node.id));

        assert!(linked_a, "fan-out node did not link to output a");
        assert!(linked_b, "fan-out node did not link to output b");
        disconnect_result.expect("disconnect output a");
        assert!(unlinked_a_after_disconnect, "output a link should be torn down");
        assert!(still_linked_b_after_disconnect, "output b link should survive disconnecting a");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::ProcessingNodePortSpec;

    fn empty_graph() -> RuntimeGraph {
        RuntimeGraph::default()
    }

    #[test]
    fn spec_to_node_derives_the_proc_prefixed_system_name() {
        let spec = ProcessingNodeSpec {
            id: "processing-mixer-game".into(),
            slug: "game".into(),
            label: "Game Mixer".into(),
            created_at: "2026-07-24T00:00:00Z".into(),
            kind: ProcessingNodeSpecKind::Mixer,
            input_sources: Vec::new(),
            output_targets: Vec::new(),
        };
        let node = processing_node_from_spec(&spec, &empty_graph());
        assert_eq!(node.system_name, "pipe-deck-proc-mixer-game");
        assert!(!node.live);
        assert!(node.inputs.is_empty());
    }

    #[test]
    fn spec_to_node_resolves_wired_ports_against_the_live_graph() {
        use crate::core::models::{Device, DeviceDirection, DeviceKind};

        let mut graph = empty_graph();
        graph.devices.push(Device {
            id: "device-headset".into(),
            system_name: "alsa_input.headset".into(),
            label: "Headset".into(),
            kind: DeviceKind::Physical,
            direction: DeviceDirection::Input,
            sink_mode: None,
            virtual_role: None,
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        });

        let spec = ProcessingNodeSpec {
            id: "processing-mixer-game".into(),
            slug: "game".into(),
            label: "Game Mixer".into(),
            created_at: "2026-07-24T00:00:00Z".into(),
            kind: ProcessingNodeSpecKind::Mixer,
            input_sources: vec![ProcessingNodePortSpec {
                source_system_name: "alsa_input.headset".into(),
                gain_percent: 80,
                muted: false,
            }],
            output_targets: Vec::new(),
        };
        let node = processing_node_from_spec(&spec, &graph);
        assert_eq!(node.inputs.len(), 1);
        assert_eq!(node.inputs[0].connected_id.as_deref(), Some("device-headset"));
        assert!(matches!(
            node.kind,
            ProcessingNodeKind::Mixer { ref input_gains_percent } if input_gains_percent == &vec![80]
        ));
    }
}
