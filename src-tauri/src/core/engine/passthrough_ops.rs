use crate::config::ConfigStore;
use crate::core::models::{ApplyResult, DeviceDirection, DeviceKind, MixSourceSpec};

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
        // no need to insert another sink in front of it. Anything else
        // (a real hardware output) gets its own dedicated virtual sink so the
        // stream keeps playing there unchanged while we get a tappable
        // monitor to feed the mic from.
        let mix_source_device_id = match &current_target_device {
            Some(device) if device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Output => {
                device.id.clone()
            }
            _ => {
                let split_label = format!("{} passthrough", stream.app_name);
                let result = self.create_virtual_output(&split_label)?;
                self.set_stream_target(stream_id, &result.device_id)?;
                self.set_device_targets(&result.device_id, &[original_target_id])?;
                result.device_id
            }
        };

        self.add_mic_mix_source(mic_device_id, &mix_source_device_id)
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
