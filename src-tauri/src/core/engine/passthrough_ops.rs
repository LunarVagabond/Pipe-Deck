use crate::core::models::{ApplyResult, DeviceDirection, DeviceKind, MixSource};

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Duplicates a playback stream's audio into a virtual mic, Soundux-style
    /// (the stream keeps playing at its original destination too), by adding
    /// the stream's own virtual output sink as a mix source of the mic. This
    /// reuses the exact same per-pair feed-sink mechanism as physical-mic
    /// mixing (`set_virtual_mic_mix`), which gives independent volume *and*
    /// mute for this one passthrough leg — muting it never touches the
    /// stream's own route or the mic's other sources.
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

        let mut updated_sources = mic.mix_sources.clone();
        if !updated_sources.iter().any(|source| source.device_id == mix_source_device_id) {
            updated_sources.push(MixSource {
                device_id: mix_source_device_id,
                volume_percent: 100,
                muted: false,
            });
        }

        self.set_virtual_mic_mix(mic_device_id, &updated_sources)
    }

    // Removing a passthrough leg needs no dedicated op: once added, it's a
    // normal mix source on the mic (see `set_virtual_mic_mix`), so dropping
    // it — or muting it without dropping it — reuses that same mix-source
    // machinery and the routing graph's existing mic-mix disconnect gesture.
}
