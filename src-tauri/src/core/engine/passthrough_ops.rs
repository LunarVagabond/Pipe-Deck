use crate::core::models::{ApplyResult, DeviceDirection, DeviceKind, SinkMode};

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Duplicates a playback stream's audio into a virtual mic, Soundux-style
    /// (the stream keeps playing at its original destination too). Reuses the
    /// existing `pipe-deck-split-*` multi-output fan-out mechanism, which
    /// already permits a virtual input as one of its targets.
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

        let split_sink_id = match &current_target_device {
            Some(device) if device.kind == DeviceKind::Virtual && device.sink_mode == Some(SinkMode::Multi) => {
                device.id.clone()
            }
            _ => {
                let split_label = format!("{} passthrough", stream.app_name);
                let result = self.create_virtual_multi_output(&split_label)?;
                self.set_stream_target(stream_id, &result.device_id)?;
                result.device_id
            }
        };

        let existing_targets = self.device_targets(&split_sink_id);
        let mut next_targets = existing_targets;
        if !next_targets.contains(&original_target_id) {
            next_targets.push(original_target_id);
        }
        if !next_targets.iter().any(|id| id == mic_device_id) {
            next_targets.push(mic_device_id.to_string());
        }

        self.set_device_targets(&split_sink_id, &next_targets)
    }

    // Removing a mic from an existing passthrough fan-out needs no dedicated
    // op: the split sink is a normal `pipe-deck-split-*` multi-output device,
    // so dropping the mic from its target list is already handled by the
    // generic `set_device_targets` path (same one the routing graph's
    // device-to-device edge disconnect already uses for any multi-sink).

    fn device_targets(&self, device_id: &str) -> Vec<String> {
        self.graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .map(|device| {
                if !device.current_targets.is_empty() {
                    device.current_targets.clone()
                } else {
                    device.current_target.clone().into_iter().collect()
                }
            })
            .unwrap_or_default()
    }
}
