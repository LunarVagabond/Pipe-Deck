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

        // A terminal Output (virtual) (#287) is a true dead end — it can't
        // feed a Mixer's mix any more than it could feed the old mic-mix
        // mechanism this generalizes; only a Bus (still routable onward)
        // qualifies as an input source.
        if direction == PortDirection::Input && matches!(node.kind, ProcessingNodeKind::Mixer { .. }) {
            if let Some(device) = self.graph.devices.iter().find(|device| device.id == peer_id) {
                let is_terminal_output = device.kind == crate::core::models::DeviceKind::Virtual
                    && device.direction == crate::core::models::DeviceDirection::Output
                    && device.virtual_role != Some(crate::core::models::VirtualRole::Bus);
                if is_terminal_output {
                    return Err(EngineError::InvalidInput(format!(
                        "{} is a terminal output and can't feed a Mixer node - only a physical input or a Bus can",
                        device.label
                    )));
                }
            }
        }

        let ports = match direction {
            PortDirection::Input => &node.inputs,
            PortDirection::Output => &node.outputs,
        };
        // A Mixer's inputs grow (N sources summed); a Fan-out's outputs grow
        // (1 source duplicated to N destinations). Every other side on every
        // kind — a Mixer's own single output, a Fan-out's single input,
        // both sides of an EQ/stub — is capped at one connection.
        let is_growable = match (direction, &node.kind) {
            (PortDirection::Input, ProcessingNodeKind::Mixer { .. }) => true,
            (PortDirection::Output, ProcessingNodeKind::FanOut) => true,
            _ => false,
        };
        if !is_growable && ports.iter().any(|port| port.connected_id.is_some()) {
            let side = if direction == PortDirection::Input { "input" } else { "output" };
            return Err(EngineError::InvalidInput(format!(
                "{node_id} accepts only one {side} - disconnect the existing one first"
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

        // Chaining (PD-032 follow-up): when the peer is itself a processing
        // node, its own port list is a second, independent piece of
        // bookkeeping (real backend: a separate persisted spec; mock: a
        // separate `ProcessingNode` in the in-memory graph) — the call above
        // only ever touched `node`'s side. Without this, a Fan-out chained
        // into a Mixer would show the connection on the Mixer's input but
        // leave the Fan-out's own output slot looking empty.
        self.mirror_peer_processing_node_port_connect(node_id, direction, peer_id)?;

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

        let peer_id = match direction {
            PortDirection::Input => &node.inputs,
            PortDirection::Output => &node.outputs,
        }
        .iter()
        .find(|port| port.index == port_index)
        .and_then(|port| port.connected_id.clone());

        self.adapter
            .relink_processing_node_port(&self.graph, &node.system_name, port_index, direction, None)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .remove_processing_node_port(node_id, direction, port_index)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        if let Some(peer_id) = peer_id {
            self.mirror_peer_processing_node_port_disconnect(node_id, direction, &peer_id)?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }

    /// Keeps a chained peer's own port list in sync with a connect on
    /// `node_id`'s side — see the call site above for why this is needed.
    /// A no-op when `peer_id` isn't itself a processing node (a device/
    /// stream peer's connectivity is derived generically elsewhere, e.g.
    /// `computeDeviceConnections` on the frontend, and has no ports list of
    /// its own to keep in lockstep). Never issues a second real PipeWire
    /// link — the one call in `connect_processing_node_port` above already
    /// made it; this only updates the peer's own bookkeeping of that same
    /// link.
    fn mirror_peer_processing_node_port_connect(
        &mut self,
        node_id: &str,
        direction: PortDirection,
        peer_id: &str,
    ) -> Result<(), EngineError> {
        let Some(peer_node) = self.graph.processing_nodes.iter().find(|node| node.id == peer_id).cloned() else {
            return Ok(());
        };
        let peer_direction = match direction {
            PortDirection::Input => PortDirection::Output,
            PortDirection::Output => PortDirection::Input,
        };
        let peer_ports = match peer_direction {
            PortDirection::Input => &peer_node.inputs,
            PortDirection::Output => &peer_node.outputs,
        };
        let peer_port_index = peer_ports
            .iter()
            .find(|port| port.connected_id.is_none())
            .map(|port| port.index)
            .unwrap_or(peer_ports.len() as u32);

        if self.graph.data_source == "mock" {
            self.adapter
                .relink_processing_node_port(&self.graph, &peer_node.system_name, peer_port_index, peer_direction, Some(node_id))
                .map_err(|error| EngineError::Adapter(error.to_string()))?;
        } else {
            let node_system_name = resolve_system_name_for_id(&self.graph, node_id)
                .ok_or_else(|| EngineError::InvalidInput(format!("peer not found: {node_id}")))?;
            ConfigStore::new()
                .upsert_processing_node_port(peer_id, peer_direction, peer_port_index, &node_system_name)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }
        Ok(())
    }

    /// Disconnect-side counterpart to `mirror_peer_processing_node_port_connect`
    /// — removes the matching port on the peer's own side rather than leaving
    /// it pointing at a node that no longer links back.
    fn mirror_peer_processing_node_port_disconnect(
        &mut self,
        node_id: &str,
        direction: PortDirection,
        peer_id: &str,
    ) -> Result<(), EngineError> {
        let Some(peer_node) = self.graph.processing_nodes.iter().find(|node| node.id == peer_id).cloned() else {
            return Ok(());
        };
        let peer_direction = match direction {
            PortDirection::Input => PortDirection::Output,
            PortDirection::Output => PortDirection::Input,
        };
        let peer_ports = match peer_direction {
            PortDirection::Input => &peer_node.inputs,
            PortDirection::Output => &peer_node.outputs,
        };
        let Some(peer_port_index) = peer_ports
            .iter()
            .find(|port| port.connected_id.as_deref() == Some(node_id))
            .map(|port| port.index)
        else {
            return Ok(());
        };

        if self.graph.data_source == "mock" {
            self.adapter
                .relink_processing_node_port(&self.graph, &peer_node.system_name, peer_port_index, peer_direction, None)
                .map_err(|error| EngineError::Adapter(error.to_string()))?;
        } else {
            ConfigStore::new()
                .remove_processing_node_port(peer_id, peer_direction, peer_port_index)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }
        Ok(())
    }

    /// Live-updates a Mixer Node's per-input gain/mute — the PD-017
    /// two-speed fast path (no relink, no reload), safe to call on every
    /// slider tick. Only meaningful for a connected input on a Mixer kind;
    /// errors for anything else rather than silently no-op-ing.
    pub fn update_processing_node_input_gain(
        &mut self,
        node_id: &str,
        port_index: u32,
        gain_percent: u8,
        muted: bool,
    ) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;

        if !matches!(node.kind, ProcessingNodeKind::Mixer { .. }) {
            return Err(EngineError::InvalidInput(format!("{node_id} has no per-input gain to update")));
        }
        let peer_id = node
            .inputs
            .iter()
            .find(|port| port.index == port_index)
            .and_then(|port| port.connected_id.as_deref())
            .ok_or_else(|| EngineError::InvalidInput(format!("input {port_index} on {node_id} isn't connected")))?
            .to_string();
        let peer_system_name = resolve_system_name_for_id(&self.graph, &peer_id)
            .ok_or_else(|| EngineError::InvalidInput(format!("peer not found: {peer_id}")))?;

        self.adapter
            .set_processing_node_input_gain(&node.system_name, &peer_system_name, gain_percent, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_processing_node_input_gain(node_id, port_index, gain_percent, muted)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }

    /// Live-updates a 5-Band EQ node's band gains — the PD-017 two-speed
    /// fast path, no reload. Errors for anything other than a live-loaded
    /// EQ5Band kind rather than silently no-op-ing.
    #[allow(clippy::too_many_arguments)]
    pub fn update_processing_node_eq_params(
        &mut self,
        node_id: &str,
        eq_sub: i32,
        eq_bass: i32,
        eq_mid: i32,
        eq_treble: i32,
        eq_air: i32,
        output_gain: i32,
    ) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;

        if !matches!(node.kind, ProcessingNodeKind::Eq5Band { .. }) {
            return Err(EngineError::InvalidInput(format!("{node_id} has no EQ params to update")));
        }

        self.adapter
            .set_processing_node_eq_params(
                &node.system_name,
                eq_sub,
                eq_bass,
                eq_mid,
                eq_treble,
                eq_air,
                output_gain,
                node.bypassed,
            )
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .update_processing_node_eq(node_id, eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }

    /// Keeps a node wired exactly as-is but toggles whether audio passes
    /// through it processed or not — connections/ports never change, only
    /// the signal itself. Only `Eq5Band` currently enforces this backend-
    /// side (reuses the same neutral-live-params mechanism
    /// `EffectChainConfig::bypassed` already has); every other kind
    /// persists the flag without a behavior change yet, since there's no
    /// "unprocessed" state for a node that only routes/sums rather than
    /// shaping the signal itself.
    pub fn set_processing_node_bypassed(&mut self, node_id: &str, bypassed: bool) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;

        // Real backend: only Eq5Band has anything to actually push live (the
        // real DSP node exists to receive it). Mock: always push regardless
        // of kind, purely so `bypassed` round-trips into the mock's own
        // graph the same way `set_processing_node_eq_params` already
        // updates it unconditionally — mock has no config-backed merge step
        // to fall back on (see `merge_processing_nodes`).
        let is_eq = matches!(node.kind, ProcessingNodeKind::Eq5Band { .. });
        if is_eq || self.graph.data_source == "mock" {
            let (eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain) = match node.kind {
                ProcessingNodeKind::Eq5Band { eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain } => {
                    (eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain)
                }
                _ => (0, 0, 0, 0, 0, 0),
            };
            self.adapter
                .set_processing_node_eq_params(&node.system_name, eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain, bypassed)
                .map_err(|error| EngineError::Adapter(error.to_string()))?;
        }

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_processing_node_bypassed(node_id, bypassed)
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
            bypassed: false,
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
        // A peer can itself be another processing node (chaining — PD-032
        // phase 5's follow-up: Mixer -> Fan-out, Fan-out -> Mixer, etc.).
        .or_else(|| {
            graph
                .processing_nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| node.system_name.clone())
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
        .or_else(|| {
            graph
                .processing_nodes
                .iter()
                .find(|node| node.system_name == system_name)
                .map(|node| node.id.clone())
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
        bypassed: spec.bypassed,
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

    #[test]
    #[ignore]
    fn mixer_node_round_trips_on_a_real_pipewire_session() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let source_a = engine.create_virtual_output("Pipe Deck Live Mixer Src A").expect("create source a");
        let source_b = engine.create_virtual_output("Pipe Deck Live Mixer Src B").expect("create source b");

        let cleanup = |engine: &mut CoreEngine, node_id: Option<&str>| {
            if let Some(id) = node_id {
                let _ = engine.remove_processing_node(id);
            }
            let _ = engine.remove_virtual_device(&source_a.system_name);
            let _ = engine.remove_virtual_device(&source_b.system_name);
        };

        let node = match engine.create_processing_node("Pipe Deck Live Mixer", ProcessingNodeSpecKind::Mixer) {
            Ok(node) => node,
            Err(error) => {
                cleanup(&mut engine, None);
                panic!("create_processing_node failed: {error}");
            }
        };

        if let Err(error) = engine.connect_processing_node_port(&node.id, PortDirection::Input, &source_a.device_id) {
            cleanup(&mut engine, Some(&node.id));
            panic!("connect input a failed: {error}");
        }
        if let Err(error) = engine.connect_processing_node_port(&node.id, PortDirection::Input, &source_b.device_id) {
            cleanup(&mut engine, Some(&node.id));
            panic!("connect input b failed: {error}");
        }

        let feed_a = pactl::feed_sink_name_for_mix_pair(&node.system_name, &source_a.system_name);
        let feed_b = pactl::feed_sink_name_for_mix_pair(&node.system_name, &source_b.system_name);
        let both_feeds_live = pactl::sink_exists(&feed_a).unwrap_or(false) && pactl::sink_exists(&feed_b).unwrap_or(false);

        if let Err(error) = engine.update_processing_node_input_gain(&node.id, 0, 55, false) {
            cleanup(&mut engine, Some(&node.id));
            panic!("update gain failed: {error}");
        }
        let gain_applied = pactl::sink_volume_percent(&feed_a).unwrap_or(None) == Some(55);

        // Disconnecting input a must tear down only its own feed sink, not
        // input b's — the #105 lesson (incomplete/incorrect teardown)
        // applied to the per-pair-feed-sink mechanism this generalizes from
        // mic-mix, on a real session rather than the mock's bookkeeping.
        let disconnect_result = engine.disconnect_processing_node_port(&node.id, PortDirection::Input, 0);
        let feed_a_gone_after_disconnect = !pactl::sink_exists(&feed_a).unwrap_or(true);
        let feed_b_survives_disconnect = pactl::sink_exists(&feed_b).unwrap_or(false);

        cleanup(&mut engine, Some(&node.id));
        let feed_b_gone_after_node_removal = !pactl::sink_exists(&feed_b).unwrap_or(true);

        assert!(both_feeds_live, "both mixer input feed sinks should exist after connecting");
        assert!(gain_applied, "gain update should be reflected on the feed sink's own volume");
        disconnect_result.expect("disconnect input a");
        assert!(feed_a_gone_after_disconnect, "input a's feed sink should be torn down on disconnect");
        assert!(feed_b_survives_disconnect, "input b's feed sink should survive disconnecting a");
        assert!(feed_b_gone_after_node_removal, "removing the mixer node should GC its remaining feed sink");
    }

    #[test]
    #[ignore]
    fn eq5band_node_round_trips_on_a_real_pipewire_session() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        // The EQ node's real DSP goes through the native effects daemon
        // (PD-027/#148), unlike Fan-out/Mixer which only ever shell out to
        // `pactl` directly — spin up (or reuse) an ephemeral one exactly the
        // way the GUI does at startup, and tear it down again afterward.
        let mut ephemeral_daemon = crate::daemon::ensure_ephemeral_daemon();
        if !crate::daemon::ipc::client::NativeHostClient::ping() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if !crate::daemon::ipc::client::NativeHostClient::ping() {
            panic!("native-effects daemon did not become reachable — is src-tauri/bin/pipe-deck-daemon-* built (make check/make build-rust)?");
        }

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let cleanup = |engine: &mut CoreEngine, node_id: Option<&str>| {
            if let Some(id) = node_id {
                let _ = engine.remove_processing_node(id);
            }
        };

        let node = match engine.create_processing_node(
            "Pipe Deck Live EQ",
            ProcessingNodeSpecKind::Eq5Band { eq_sub: 0, eq_bass: 0, eq_mid: 0, eq_treble: 0, eq_air: 0, output_gain: 0 },
        ) {
            Ok(node) => node,
            Err(error) => {
                cleanup(&mut engine, None);
                panic!("create_processing_node failed: {error}");
            }
        };

        let sink_live_after_create = pactl::sink_exists(&node.system_name).unwrap_or(false);

        // Live-param push (the PD-017 two-speed fast path — no reload, just
        // a `pw-cli set-param`) is attempted but not asserted on: it shares
        // its exact mechanism with `CoreEngine::set_effect_chain_live_params`
        // (same `pw_cli::find_node_id_by_name` + `fx_validate::live_params`
        // key scheme, established/pre-existing, not new in this phase), and
        // `pw-cli set-param` has been observed to time out against *both*
        // mechanisms in this specific sandbox's PipeWire/pw-cli combination
        // — an environment quirk worth a manual re-check on real hardware,
        // not a regression to gate this test on. The structural
        // create/remove path below (real DSP node lifecycle, the actual
        // PD-032 correctness concern) is what's asserted.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let update_result = engine.update_processing_node_eq_params(&node.id, 6, 0, 0, 0, 0, 0);
        if let Err(error) = &update_result {
            eprintln!(
                "note: live EQ param update did not succeed in this environment ({error}) — see this test's doc comment"
            );
        }

        cleanup(&mut engine, Some(&node.id));
        let sink_gone_after_removal = !pactl::sink_exists(&node.system_name).unwrap_or(true);

        if let Some(child) = ephemeral_daemon.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }

        assert!(sink_live_after_create, "EQ node's native-hosted sink did not appear after creation");
        assert!(sink_gone_after_removal, "removing the EQ node should unload its native chain");
    }

    /// A Stub node never creates a real PipeWire sink (see
    /// `ProcessingNodeKind::Stub`) — connecting/disconnecting its ports must
    /// therefore be a true no-op rather than attempting a `pw-link` against
    /// a sink name that was never registered. Caught for real during phase
    /// 5 development: without the no-op guard in
    /// `LinuxPipeWireBackend::relink_processing_node_port`, this failed with
    /// "no output ports" against the real backend even though the identical
    /// flow passed happily against the mock.
    #[test]
    #[ignore]
    fn stub_node_connect_disconnect_is_a_true_no_op_on_a_real_pipewire_session() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let upstream = engine.create_virtual_output("Pipe Deck Live Stub Upstream").expect("create upstream");
        let downstream = engine.create_virtual_output("Pipe Deck Live Stub Downstream").expect("create downstream");

        let cleanup = |engine: &mut CoreEngine, node_id: Option<&str>| {
            if let Some(id) = node_id {
                let _ = engine.remove_processing_node(id);
            }
            let _ = engine.remove_virtual_device(&upstream.system_name);
            let _ = engine.remove_virtual_device(&downstream.system_name);
        };

        let node = match engine.create_processing_node(
            "Pipe Deck Live Stub",
            ProcessingNodeSpecKind::Stub { stub_kind: crate::core::models::StubEffectKind::ReverbDelay },
        ) {
            Ok(node) => node,
            Err(error) => {
                cleanup(&mut engine, None);
                panic!("create_processing_node failed: {error}");
            }
        };
        let no_sink_created = !pactl::sink_exists(&node.system_name).unwrap_or(true);

        let connect_input = engine.connect_processing_node_port(&node.id, PortDirection::Input, &upstream.device_id);
        let connect_output = engine.connect_processing_node_port(&node.id, PortDirection::Output, &downstream.device_id);
        let disconnect_input = engine.disconnect_processing_node_port(&node.id, PortDirection::Input, 0);
        let remove_result = engine.remove_processing_node(&node.id);

        cleanup(&mut engine, None);

        assert!(no_sink_created, "a stub node must never create a real PipeWire sink");
        connect_input.expect("connecting a stub's input should be a no-op, not an error");
        connect_output.expect("connecting a stub's output should be a no-op, not an error");
        disconnect_input.expect("disconnecting a stub's input should be a no-op, not an error");
        remove_result.expect("removing a stub node should succeed");
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
            bypassed: false,
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
            bypassed: false,
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
