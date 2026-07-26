use crate::backend::slugify;
use crate::config::ConfigStore;
use crate::core::models::{
    ApplyResult, PortDirection, ProcessingNode, ProcessingNodeKind, ProcessingNodePort,
    ProcessingNodeSpec, ProcessingNodeSpecKind, RuntimeGraph,
};
use chrono::Utc;

use super::{CoreEngine, EngineError};

/// A Mixer's inputs grow (N sources summed); a Fan-out's outputs grow (1
/// source duplicated to N destinations). Every other side on every kind — a
/// Mixer's own single output, a Fan-out's single input, both sides of an
/// EQ/stub — is capped at one connection. Shared by `connect_processing_node_port`
/// (checking the node the caller named) and its peer-mirroring counterpart
/// (checking the *other* end of a node-to-node chain the same way), so a
/// chain connect can't leave one side's single port over-subscribed just
/// because only one end of the pair was ever validated.
fn processing_node_port_growable(direction: PortDirection, kind: &ProcessingNodeKind) -> bool {
    matches!(
        (direction, kind),
        (PortDirection::Input, ProcessingNodeKind::Mixer { .. }) | (PortDirection::Output, ProcessingNodeKind::FanOut { .. })
    )
}

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
        let is_growable = processing_node_port_growable(direction, &node.kind);
        if !is_growable && ports.iter().any(|port| port.connected_id.is_some()) {
            let side = if direction == PortDirection::Input { "input" } else { "output" };
            return Err(EngineError::InvalidInput(format!(
                "{node_id} accepts only one {side} - disconnect the existing one first"
            )));
        }

        // The peer's own opposite-direction port list needs the same
        // capacity check — without this, chaining into an already-occupied
        // single-capacity peer port (e.g. a second node trying to feed an
        // EQ's one input) silently corrupts the peer's bookkeeping instead
        // of being rejected (`mirror_peer_processing_node_port_connect`
        // below relies on this having already been validated).
        if let Some(peer_node) = self.graph.processing_nodes.iter().find(|n| n.id == peer_id) {
            let peer_direction = match direction {
                PortDirection::Input => PortDirection::Output,
                PortDirection::Output => PortDirection::Input,
            };
            let peer_ports = match peer_direction {
                PortDirection::Input => &peer_node.inputs,
                PortDirection::Output => &peer_node.outputs,
            };
            let peer_growable = processing_node_port_growable(peer_direction, &peer_node.kind);
            if !peer_growable && peer_ports.iter().any(|port| port.connected_id.as_deref() != Some(node_id) && port.connected_id.is_some())
            {
                let side = if peer_direction == PortDirection::Input { "input" } else { "output" };
                return Err(EngineError::InvalidInput(format!(
                    "{peer_id} accepts only one {side} - disconnect the existing one first"
                )));
            }
        }

        // A device/stream peer can only ever be genuinely wired into one
        // place at a time — moving it onto a fresh sink-input/feed-sink
        // target implicitly abandons wherever it was previously plugged in,
        // the same way `pactl move-sink-input` only ever has one live
        // destination. Without this check, dragging the same stream onto a
        // second processing node's input leaves the *first* node's port
        // bookkeeping (and its own PipeWire-side feed sink) stale — still
        // shown as connected and still gain-controlled by a slider, even
        // though the peer's audio has actually moved elsewhere. Mirrors the
        // same "disconnect the stale side first" principle bb25d6d already
        // applies to an edge_update retarget, generalized here to cover a
        // brand-new connect gesture landing on a different node entirely.
        // Only applies to device/stream peers — a processing-node peer's own
        // output-side capacity is already enforced by the peer-capacity
        // check above (non-growable single-output kinds reject a second
        // connect outright).
        if self.graph.processing_nodes.iter().find(|n| n.id == peer_id).is_none() {
            let stale: Vec<(String, u32)> = self
                .graph
                .processing_nodes
                .iter()
                .filter(|other| other.id != node_id)
                .flat_map(|other| {
                    let other_ports = match direction {
                        PortDirection::Input => &other.inputs,
                        PortDirection::Output => &other.outputs,
                    };
                    other_ports
                        .iter()
                        .filter(|port| port.connected_id.as_deref() == Some(peer_id))
                        .map(|port| (other.id.clone(), port.index))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (stale_node_id, stale_port_index) in stale {
                self.disconnect_processing_node_port(&stale_node_id, direction, stale_port_index)?;
            }
        }

        // Re-borrow: the stale-disconnect pass above (if it ran) mutated
        // `self.graph` via `refresh_graph()`, so `node`'s port list captured
        // before it may be out of date — recompute the insertion index
        // against current state rather than the possibly-stale `ports`
        // local.
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;
        let ports = match direction {
            PortDirection::Input => &node.inputs,
            PortDirection::Output => &node.outputs,
        };
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
        // The caller (`connect_processing_node_port`) has already validated
        // capacity on this exact peer/direction before making any live or
        // config change, so a non-growable peer either has a free slot
        // already (nothing connected yet) or is being re-pointed at the same
        // `node_id` it's already wired to — either way, reuse its one port
        // index rather than the growable path's "append past the end",
        // which would otherwise leave a non-growable peer's port list with
        // more entries than its live PipeWire object actually has.
        let peer_growable = processing_node_port_growable(peer_direction, &peer_node.kind);
        let peer_port_index = if peer_growable {
            peer_ports
                .iter()
                .find(|port| port.connected_id.is_none())
                .map(|port| port.index)
                .unwrap_or(peer_ports.len() as u32)
        } else {
            peer_ports.first().map(|port| port.index).unwrap_or(0)
        };

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
        // Must match whatever `relink_processing_node_port`'s Mixer-input arm
        // (`live.rs`) actually named the feed sink at connect time. For a
        // stream peer, that's the raw graph `id` (the stream's own
        // `system_name` may still be unresolved at connect time — see the
        // comment on that arm), NOT `stream.system_name`, which is what
        // `resolve_system_name_for_id` returns for a stream. Using the
        // resolved system_name here computed a *different* feed-sink name
        // than the one connect actually created, so the gain/mute update
        // silently targeted a nonexistent sink while the real one kept
        // carrying audio at unity gain. Device/processing-node peers are
        // unaffected — connect time already uses their system_name too.
        let peer_feed_key = if self.graph.streams.iter().any(|stream| stream.id == peer_id) {
            peer_id.clone()
        } else {
            resolve_system_name_for_id(&self.graph, &peer_id)
                .ok_or_else(|| EngineError::InvalidInput(format!("peer not found: {peer_id}")))?
        };

        self.adapter
            .set_processing_node_input_gain(&node.system_name, &peer_feed_key, gain_percent, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_processing_node_input_gain(node_id, port_index, gain_percent, muted)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }

    /// Live-updates a Fan-Out node's own output volume/mute — a plain
    /// device-style volume, not a shaping gain (Fan-Out has no DSP; see
    /// `ProcessingNodeKind::FanOut` field doc). Addressed by `system_name`
    /// directly rather than through a `Device`, same as
    /// `update_processing_node_input_gain`.
    pub fn update_processing_node_volume(
        &mut self,
        node_id: &str,
        volume_percent: u8,
        muted: bool,
    ) -> Result<ApplyResult, EngineError> {
        let node = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("processing node not found: {node_id}")))?;

        if !matches!(node.kind, ProcessingNodeKind::FanOut { .. }) {
            return Err(EngineError::InvalidInput(format!("{node_id} has no volume to update")));
        }

        self.adapter
            .set_processing_node_volume(&node.system_name, volume_percent, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_processing_node_volume(node_id, volume_percent, muted)
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

        // Unlike the device-attached EQ's Live Params fast path
        // (`effects_ops.rs::set_effect_chain_live_params`), this function is
        // the *only* place Eq5Band values ever get persisted — there's no
        // separate "Structural Apply" step for a processing node. A slider
        // drag racing ahead of the node actually registering as loaded
        // (`pw_cli::find_node_id_by_name` in the live backend can lag behind
        // `NativeHostClient::is_loaded`, which is what the UI's `node.live`
        // reflects) is an expected transient state, same as the device path
        // — but unlike that path, we must still persist here, or the user's
        // dragged value is silently lost and the slider reverts to whatever
        // was last saved (0, for a never-yet-applied node).
        let live_apply_error = self
            .adapter
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
            .err();

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .update_processing_node_eq(node_id, eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;

        if let Some(error) = live_apply_error {
            return Ok(ApplyResult { success: false, message: Some(error.to_string()) });
        }
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
///
/// **Test coverage gap, not just a code comment**: this early return means
/// no mock-backed `tests/mock_backend_integration.rs` test can ever execute
/// this function's actual body — including the from-scratch batch-conversion
/// ordering that caused a real live-only bug (a processing-node-to-
/// processing-node port could never resolve, since `graph.processing_nodes`
/// is empty/stale at the time each spec converts; see
/// `processing_node_from_spec_with_siblings`'s doc comment). The closest
/// coverage is the unit test `spec_to_node_resolves_a_processing_node_peer_via_the_sibling_map`,
/// which calls the sibling-resolution helper directly with a hand-built
/// `specs`/`siblings` map — real, but it doesn't exercise this function
/// itself, `ConfigStore`, or a real `AudioBackend`. Any future change here
/// needs a live/manual check; nothing automated will catch a regression in
/// this function's own batching logic.
pub(super) fn merge_processing_nodes(graph: &mut RuntimeGraph, adapter: &dyn crate::backend::AudioBackend) {
    if graph.data_source == "mock" {
        return;
    }

    let specs = ConfigStore::new().processing_nodes();
    // A port referencing another processing node (Mixer -> Fan-Out chaining
    // etc.) can't be resolved against `graph.processing_nodes` here — that
    // field is empty/stale at this point, since this loop is what's about
    // to (re)populate it from scratch, and `graph` isn't updated until every
    // spec has already been converted. Without this, a node-to-node
    // connection's `connected_id` came back `None` on every single live
    // refresh (confirmed live: `pipe-deck graph` showed both sides of a
    // real Mixer -> Fan-Out link with no `connected_id` at all despite both
    // being correctly persisted in config.yaml) — never reproduced by the
    // mock backend, which bypasses this whole spec-reconstruction path and
    // returns its own already-consistent in-memory state directly. Resolve
    // processing-node peers against this sibling map (built from the same
    // `specs` this loop is converting) instead.
    let sibling_ids: std::collections::HashMap<String, String> = specs
        .iter()
        .map(|spec| (processing_node_system_name(spec), spec.id.clone()))
        .collect();
    let nodes: Vec<ProcessingNode> = specs
        .iter()
        .map(|spec| {
            let mut node = processing_node_from_spec_with_siblings(spec, graph, &sibling_ids);
            node.live = adapter.is_processing_node_loaded(&node.system_name);
            node
        })
        .collect();
    graph.processing_nodes = nodes;
}

fn processing_node_system_name(spec: &ProcessingNodeSpec) -> String {
    format!("pipe-deck-proc-{}-{}", spec_kind_slug(&spec.kind), spec.slug)
}

fn spec_kind_slug(kind: &ProcessingNodeSpecKind) -> &'static str {
    match kind {
        ProcessingNodeSpecKind::Mixer => "mixer",
        ProcessingNodeSpecKind::FanOut { .. } => "fan_out",
        ProcessingNodeSpecKind::Eq5Band { .. } => "eq5band",
        ProcessingNodeSpecKind::Stub { .. } => "stub",
    }
}

/// Synthetic prefix for a stream peer's persisted identity — see
/// `resolve_system_name_for_id`'s doc comment for why a stream can't use its
/// real `system_name` here the way a device safely can.
const STREAM_PEER_PREFIX: &str = "pipe-deck-stream-";

/// A device's `system_name` is a real, stable PipeWire identity — safe to
/// persist and re-resolve later. A *stream*'s `system_name` (PipeWire
/// `node.name`) is not: multiple simultaneous streams from the same app
/// (e.g. two Firefox tabs both playing audio) commonly report the exact
/// same `node.name`. Persisting that shared string as a Mixer input port's
/// identity means every such stream re-resolves to whichever one happens to
/// be first in the live list on the next refresh — the other looks
/// permanently unconnected (no port, no gain slider, no rendered handle for
/// its edge to land on) even though it's genuinely wired in. `Stream.id`
/// (the live PipeWire object id, e.g. `"node-42"`) is unique per instance
/// within the session, so a stream peer's identity is persisted as a
/// synthetic `"pipe-deck-stream-{id}"` string instead — resolved back by
/// `resolve_id_for_system_name` recognizing the prefix, never by matching
/// against `Stream.system_name`. This never reaches live PipeWire calls
/// (those resolve a stream peer by id directly, see `live.rs`'s Mixer input
/// arm), so it only affects this persistence/reconstruction layer.
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
                .map(|stream| format!("{STREAM_PEER_PREFIX}{}", stream.id))
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

fn resolve_id_for_system_name(
    graph: &RuntimeGraph,
    system_name: &str,
    siblings: &std::collections::HashMap<String, String>,
) -> Option<String> {
    if let Some(stream_id) = system_name.strip_prefix(STREAM_PEER_PREFIX) {
        return graph
            .streams
            .iter()
            .find(|stream| stream.id == stream_id)
            .map(|stream| stream.id.clone());
    }
    graph
        .devices
        .iter()
        .find(|device| device.system_name == system_name)
        .map(|device| device.id.clone())
        .or_else(|| {
            graph
                .processing_nodes
                .iter()
                .find(|node| node.system_name == system_name)
                .map(|node| node.id.clone())
        })
        .or_else(|| siblings.get(system_name).cloned())
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
    processing_node_from_spec_with_siblings(spec, graph, &std::collections::HashMap::new())
}

/// `siblings` is an extra `system_name -> id` lookup for processing-node
/// peers, consulted alongside `graph.processing_nodes` (see
/// `merge_processing_nodes`'s doc comment for why the latter alone isn't
/// enough during a from-scratch batch rebuild). Empty for every other
/// caller, where `graph.processing_nodes` is already complete/correct.
fn processing_node_from_spec_with_siblings(
    spec: &ProcessingNodeSpec,
    graph: &RuntimeGraph,
    siblings: &std::collections::HashMap<String, String>,
) -> ProcessingNode {
    let kind_slug = spec_kind_slug(&spec.kind);
    let system_name = format!("pipe-deck-proc-{kind_slug}-{}", spec.slug);

    let kind = match &spec.kind {
        ProcessingNodeSpecKind::Mixer => ProcessingNodeKind::Mixer {
            input_gains_percent: spec.input_sources.iter().map(|port| port.gain_percent).collect(),
        },
        ProcessingNodeSpecKind::FanOut { volume_percent, muted } => {
            ProcessingNodeKind::FanOut { volume_percent: *volume_percent, muted: *muted }
        }
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
            connected_id: resolve_id_for_system_name(graph, &port.source_system_name, siblings),
        })
        .collect();
    let outputs = spec
        .output_targets
        .iter()
        .enumerate()
        .map(|(index, target)| ProcessingNodePort {
            index: index as u32,
            connected_id: resolve_id_for_system_name(graph, target, siblings),
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

        let node = match engine.create_processing_node(
            "Pipe Deck Live Fan-out",
            ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false },
        ) {
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

    /// Regression for a real bug found in manual live-PipeWire testing:
    /// `merge_processing_nodes` converts every persisted spec into a
    /// `ProcessingNode` in one pass, assigning `graph.processing_nodes` only
    /// after the whole batch finishes — so a port referencing *another*
    /// processing node (Mixer -> Fan-Out chaining) could never resolve
    /// against `graph.processing_nodes` during that same pass, since it was
    /// always empty/stale at the time each individual spec was converted.
    /// Confirmed live: both sides of a real Mixer -> Fan-Out connection
    /// showed `connected_id: null` despite being correctly persisted in
    /// config.yaml on both ends. Never reproduced by the mock backend,
    /// which bypasses this whole spec-reconstruction path entirely.
    #[test]
    fn spec_to_node_resolves_a_processing_node_peer_via_the_sibling_map() {
        let mixer_spec = ProcessingNodeSpec {
            id: "processing-mixer-mixer".into(),
            slug: "mixer".into(),
            label: "Mixer".into(),
            created_at: "2026-07-26T00:00:00Z".into(),
            kind: ProcessingNodeSpecKind::Mixer,
            input_sources: Vec::new(),
            output_targets: vec!["pipe-deck-proc-fan_out-fan-out".into()],
            bypassed: false,
        };
        let fan_out_spec = ProcessingNodeSpec {
            id: "processing-fan_out-fan-out".into(),
            slug: "fan-out".into(),
            label: "Fan-Out".into(),
            created_at: "2026-07-26T00:00:00Z".into(),
            kind: ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false },
            input_sources: vec![ProcessingNodePortSpec {
                source_system_name: "pipe-deck-proc-mixer-mixer".into(),
                gain_percent: 100,
                muted: false,
            }],
            output_targets: Vec::new(),
            bypassed: false,
        };

        // Mirrors `merge_processing_nodes`: build the sibling map from all
        // specs up front, then convert each spec against a `graph` whose
        // `processing_nodes` field is still empty (as it always is mid-batch).
        let siblings: std::collections::HashMap<String, String> = [&mixer_spec, &fan_out_spec]
            .into_iter()
            .map(|spec| (processing_node_system_name(spec), spec.id.clone()))
            .collect();
        let graph = empty_graph();

        let mixer_node = processing_node_from_spec_with_siblings(&mixer_spec, &graph, &siblings);
        assert_eq!(mixer_node.outputs.len(), 1);
        assert_eq!(
            mixer_node.outputs[0].connected_id.as_deref(),
            Some("processing-fan_out-fan-out"),
            "Mixer's output must resolve to the Fan-Out node, not come back unresolved"
        );

        let fan_out_node = processing_node_from_spec_with_siblings(&fan_out_spec, &graph, &siblings);
        assert_eq!(fan_out_node.inputs.len(), 1);
        assert_eq!(
            fan_out_node.inputs[0].connected_id.as_deref(),
            Some("processing-mixer-mixer"),
            "Fan-Out's input must resolve to the Mixer node, not come back unresolved"
        );
    }
}
