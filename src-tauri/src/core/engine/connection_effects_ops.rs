use crate::config::ConfigStore;
use crate::core::models::ApplyResult;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Resolves a connection source (device or stream) to the system name
    /// used for persistence, mirroring `backend::linux::connection_effects`'s
    /// own resolution — the engine can't call into `backend::linux` directly
    /// (see CLAUDE.md's AudioBackend boundary rule), so this is a small,
    /// intentional duplication of that lookup over `self.graph`, which is
    /// plain data, not backend-specific.
    fn resolve_connection_system_name(&self, id: &str) -> Option<String> {
        if let Some(device) = self.graph.devices.iter().find(|device| device.id == id) {
            return Some(device.system_name.clone());
        }
        self.graph
            .streams
            .iter()
            .find(|stream| stream.id == id)
            .map(|stream| stream.system_name.clone().unwrap_or_else(|| stream.id.clone()))
    }

    fn resolve_target_system_name(&self, target_device_id: &str) -> Result<String, EngineError> {
        self.graph
            .devices
            .iter()
            .find(|device| device.id == target_device_id)
            .map(|device| device.system_name.clone())
            .ok_or_else(|| EngineError::NotFound(format!("target device not found: {target_device_id}")))
    }

    /// Adds a `Volume` connection effect (unity gain, unmuted) to an existing
    /// connection from `source_id` (a device or stream) to `target_device_id`.
    pub fn add_connection_effect(
        &mut self,
        source_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let (source_system_name, target_system_name) = self
            .adapter
            .add_connection_effect(&self.graph, source_id, target_device_id)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_connection_effects(
                    &source_system_name,
                    &target_system_name,
                    vec![crate::core::models::ConnectionEffectKind::Volume {
                        volume_percent: 100,
                        muted: false,
                    }],
                )
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// Removes a connection's effect entirely, tearing down its backing feed
    /// sink and reverting to a direct (ungained) route.
    pub fn remove_connection_effect(
        &mut self,
        source_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let source_system_name = self
            .resolve_connection_system_name(source_id)
            .ok_or_else(|| EngineError::NotFound(format!("connection source not found: {source_id}")))?;
        let target_system_name = self.resolve_target_system_name(target_device_id)?;

        self.adapter
            .remove_connection_effect(&source_system_name, &target_system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_connection_effects(&source_system_name, &target_system_name, Vec::new())
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// Sets the gain for one already-added connection effect. Safe to call at
    /// high frequency for a live slider drag (mirrors `set_mix_source_volume`).
    pub fn set_connection_volume(
        &mut self,
        source_id: &str,
        target_device_id: &str,
        percent: u8,
    ) -> Result<ApplyResult, EngineError> {
        let source_system_name = self
            .resolve_connection_system_name(source_id)
            .ok_or_else(|| EngineError::NotFound(format!("connection source not found: {source_id}")))?;
        let target_system_name = self.resolve_target_system_name(target_device_id)?;

        self.adapter
            .set_connection_volume(&source_system_name, &target_system_name, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .update_connection_volume(&source_system_name, &target_system_name, percent)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// Mutes/unmutes a connection's effect without touching its link.
    pub fn set_connection_mute(
        &mut self,
        source_id: &str,
        target_device_id: &str,
        muted: bool,
    ) -> Result<ApplyResult, EngineError> {
        let source_system_name = self
            .resolve_connection_system_name(source_id)
            .ok_or_else(|| EngineError::NotFound(format!("connection source not found: {source_id}")))?;
        let target_system_name = self.resolve_target_system_name(target_device_id)?;

        self.adapter
            .set_connection_mute(&source_system_name, &target_system_name, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .update_connection_mute(&source_system_name, &target_system_name, muted)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }
}
