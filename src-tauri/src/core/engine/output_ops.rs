//! Default-output query/switch and the "global mute" quick action (#11) —
//! backs the tray's mute toggle and default-output submenu. "Global mute"
//! had no prior definition anywhere in this codebase (no per-app/master
//! mute concept exists outside per-device/per-stream mute), so it's defined
//! here, for the tray only, as muting the *current default output device* —
//! the same thing a user means by "mute my speakers" and the same target
//! `set_default_output_device` operates on.
use super::{CoreEngine, EngineError};
use crate::core::models::{Device, DeviceDirection};

impl CoreEngine {
    /// The device currently acting as the system default output (sink), if
    /// the backend can determine one. `None` on a backend with no such
    /// concept (`StubBackend`) or if the reported default doesn't match any
    /// device currently on the graph (e.g. a stale name mid-teardown).
    pub fn default_output_device(&self) -> Option<&Device> {
        let system_name = self.adapter.default_output_device_name()?;
        self.graph.devices.iter().find(|device| {
            device.system_name == system_name
                && matches!(device.direction, DeviceDirection::Output | DeviceDirection::Duplex)
        })
    }

    /// All devices eligible to become the default output — every
    /// Output/Duplex device currently on the graph. No "recent sinks" list
    /// exists anywhere in config/state today, so this first pass scopes
    /// "recent sinks" (per the issue) down to "all currently available
    /// output devices"; a real recency-ranked list is follow-up scope.
    pub fn available_output_devices(&self) -> Vec<&Device> {
        self.graph
            .devices
            .iter()
            .filter(|device| matches!(device.direction, DeviceDirection::Output | DeviceDirection::Duplex))
            .collect()
    }

    /// Makes `device_id` the system default output.
    pub fn set_default_output_device(&mut self, device_id: &str) -> Result<(), EngineError> {
        let system_name = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .map(|device| device.system_name.clone())
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {device_id}")))?;

        self.adapter
            .set_default_output_device(&system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    /// Whether the current default output is muted, if one can be
    /// determined — the tray mute toggle's checked state.
    pub fn default_output_muted(&self) -> Option<bool> {
        self.default_output_device().map(|device| device.muted.unwrap_or(false))
    }

    /// Toggles mute on the current default output device — the tray's
    /// "global mute" quick action (see module doc for what "global" means
    /// here).
    pub fn toggle_default_output_mute(&mut self) -> Result<(), EngineError> {
        let Some(device) = self.default_output_device() else {
            return Err(EngineError::NotFound("no default output device".into()));
        };
        let device_id = device.id.clone();
        let muted = device.muted.unwrap_or(false);
        self.set_device_mute(&device_id, !muted)
    }
}
