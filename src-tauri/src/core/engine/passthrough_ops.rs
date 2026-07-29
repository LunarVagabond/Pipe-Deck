use crate::backend::slugify;
use crate::config::ConfigStore;
use crate::core::models::{ApplyResult, DeviceDirection, DeviceKind, MixSourceSpec, PortDirection, ProcessingNodeSpecKind};

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Duplicates a playback stream's audio into a virtual mic, Soundux-style
    /// (the stream keeps playing at its original destination too), by adding
    /// the stream's own virtual output sink as a mix source of the mic. This
    /// reuses the same per-pair feed-sink mechanism the retired mic-mix UX
    /// used (`AudioBackend::apply_virtual_mic_mix`, still live — it just has
    /// no other authoring path left now that mixing itself goes through the
    /// Mixer Node), which gives independent volume *and* mute for this one
    /// passthrough leg — muting it never touches the stream's own route or
    /// the mic's other sources.
    pub fn enable_stream_mic_passthrough(
        &mut self,
        stream_id: &str,
        mic_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let stream = self
            .graph
            .streams
            .iter()
            .find(|stream| stream.id == stream_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("stream not found: {stream_id}")))?;

        let mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == mic_device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {mic_device_id}")))?;

        if mic.kind != DeviceKind::Virtual || mic.direction != DeviceDirection::Input {
            return Err(EngineError::InvalidInput(
                "passthrough target must be a virtual microphone".to_string(),
            ));
        }

        let Some(original_target_id) = stream.current_target.clone() else {
            return Err(EngineError::InvalidInput(
                "stream has no current destination to duplicate".to_string(),
            ));
        };

        if original_target_id == mic_device_id {
            return Err(EngineError::InvalidInput(
                "stream is already routed to this device".to_string(),
            ));
        }

        let current_target_device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == original_target_id)
            .cloned();

        // A device we created ourselves (any Pipe Deck virtual output, single
        // or multi) already has reliable monitor ports we can tap directly —
        // reuse the existing per-pair mix-source mechanism. Anything else
        // (most commonly a stream playing straight to a bare hardware sink,
        // with no Pipe Deck-owned monitor in front of it — issue #305, since
        // device-to-device Bus routing that used to synthesize one was
        // retired in #293) is duplicated instead by auto-provisioning a
        // Fan-Out processing node: one input (the stream itself, moved onto
        // the node's own sink) fanning out to two outputs (the stream's
        // original destination, to keep it audible there, and the mic).
        let Some(device) = &current_target_device else {
            return Err(EngineError::InvalidInput(
                "stream has no resolvable current destination to duplicate".to_string(),
            ));
        };
        if device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Output {
            let mix_source_device_id = device.id.clone();
            return self.add_mic_mix_source(mic_device_id, &mix_source_device_id);
        }

        let original_target_id = device.id.clone();
        self.enable_stream_mic_passthrough_via_fan_out(&stream.id, &stream.app_name, mic_device_id, &original_target_id)
    }

    /// Fan-Out-backed fallback for `enable_stream_mic_passthrough` when the
    /// stream's current destination has no tappable Pipe Deck monitor of its
    /// own (issue #305). One Fan-Out node per app is created lazily and
    /// reused for every stream from that app, rather than one per stream —
    /// fewer generated nodes, and it self-heals the same way a Mixer input's
    /// stream port now does (see `processing_node_ops::resolve_input_port_peer`,
    /// issue #304) if this exact stream instance later gets replaced by a
    /// same-app reload.
    fn enable_stream_mic_passthrough_via_fan_out(
        &mut self,
        stream_id: &str,
        stream_app_name: &str,
        mic_device_id: &str,
        original_target_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let label = format!("{stream_app_name} Passthrough");
        let fan_out_id = format!("processing-fan_out-{}", slugify(&label));

        let node_output_connected_to = |engine: &CoreEngine, target_id: &str| {
            engine
                .graph
                .processing_nodes
                .iter()
                .find(|node| node.id == fan_out_id)
                .is_some_and(|node| node.outputs.iter().any(|port| port.connected_id.as_deref() == Some(target_id)))
        };

        if node_output_connected_to(self, mic_device_id) {
            return Ok(ApplyResult {
                success: false,
                message: Some("This device is already mixed into this device.".to_string()),
            });
        }

        if !self.graph.processing_nodes.iter().any(|node| node.id == fan_out_id) {
            self.create_processing_node(&label, ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false })?;
        }

        // Wire both outputs before the input: connecting the input is what
        // actually moves the stream's live audio onto the Fan-Out's sink
        // (`relink_processing_node_port`'s non-Mixer input arm), so doing it
        // last keeps the window where the stream isn't audible anywhere as
        // short as possible.
        if !node_output_connected_to(self, original_target_id) {
            self.connect_processing_node_port(&fan_out_id, PortDirection::Output, original_target_id)?;
        }
        self.connect_processing_node_port(&fan_out_id, PortDirection::Output, mic_device_id)?;

        let input_already_wired = self
            .graph
            .processing_nodes
            .iter()
            .find(|node| node.id == fan_out_id)
            .is_some_and(|node| node.inputs.iter().any(|port| port.connected_id.as_deref() == Some(stream_id)));
        if !input_already_wired {
            self.connect_processing_node_port(&fan_out_id, PortDirection::Input, stream_id)?;
        }

        Ok(ApplyResult { success: true, message: None })
    }

    // Removing a passthrough leg needs no dedicated op: once added, it's a
    // normal mix source on the mic, so dropping it — or muting it without
    // dropping it — reuses `add_mic_mix_source`'s own apply path (called
    // again with the source excluded) and the routing graph's existing
    // mic-mix disconnect gesture.

    /// The one remaining mix-source authoring path, kept private now that
    /// mic-mix itself is retired in favor of the Mixer Node (PD-032) —
    /// `enable_stream_mic_passthrough` above is its only caller. Computes
    /// the resulting full mix from this engine's own graph (not a
    /// frontend-supplied list) so two calls fired close together can't race.
    fn add_mic_mix_source(&mut self, virtual_mic_device_id: &str, source_device_id: &str) -> Result<ApplyResult, EngineError> {
        let virtual_mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == virtual_mic_device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound("virtual mic not found".to_string()))?;

        if virtual_mic.mix_sources.iter().any(|source| source.device_id == source_device_id) {
            return Ok(ApplyResult {
                success: false,
                message: Some("This device is already mixed into this device.".to_string()),
            });
        }

        let source = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == source_device_id)
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {source_device_id}")))?;

        let mut mix_source_specs: Vec<MixSourceSpec> = virtual_mic
            .mix_sources
            .iter()
            .filter_map(|existing| {
                let system_name = self
                    .graph
                    .devices
                    .iter()
                    .find(|device| device.id == existing.device_id)
                    .map(|device| device.system_name.clone())?;
                Some(MixSourceSpec { system_name, volume_percent: existing.volume_percent, muted: existing.muted })
            })
            .collect();
        mix_source_specs.push(MixSourceSpec {
            system_name: source.system_name.clone(),
            volume_percent: 100,
            muted: false,
        });

        self.adapter
            .apply_virtual_mic_mix(&virtual_mic, &mix_source_specs)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_virtual_mic_mix_sources(&virtual_mic.system_name, &mix_source_specs)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult { success: true, message: None })
    }
}
