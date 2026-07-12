use crate::config::ConfigStore;
use crate::core::effects::apply_profile_effects;
use crate::core::models::{
    ApplyResult, Profile, ProfileIndexEntry, RoutingDrift,
};
use crate::core::profile::{capture_profile_from_graph, update_profile_timestamp};
use crate::core::profile_drift::compare_profile_to_graph;
use crate::core::routing::{
    apply_profile_routing, apply_profile_volumes, capture_routing_snapshot,
    restore_routing_snapshot,
};
use crate::config::profile_store::{import_profile_archive, ProfileStore};
use crate::core::restore;
use std::path::Path;

use super::mock::{apply_mock_profile, apply_mock_profile_volumes, apply_mock_snapshot};
use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn get_profile_drift(&self, profile_id: &str) -> Result<RoutingDrift, EngineError> {
        let profile = self.get_profile(profile_id)?;
        Ok(compare_profile_to_graph(&profile, &self.graph))
    }

    pub fn apply_profile_routes(&mut self, profile_id: &str) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let profile = self.get_profile(profile_id)?;
        let snapshot = capture_routing_snapshot(&self.graph);

        let apply_result = if self.graph.data_source == "mock" {
            apply_mock_profile(&mut self.graph, &profile)
        } else {
            apply_profile_routing(&self.graph, &profile)
                .map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = apply_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn get_profile(&self, profile_id: &str) -> Result<Profile, EngineError> {
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());
        profile_store
            .load_profile_by_id(profile_id, &config.profile_index)
            .map_err(|error| EngineError::Profile(error.to_string()))
    }

    pub fn save_profile(
        &mut self,
        profile_id: &str,
        name: Option<String>,
    ) -> Result<Profile, EngineError> {
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());

        let entry = config
            .profile_index
            .iter()
            .find(|entry| entry.id == profile_id)
            .cloned()
            .ok_or_else(|| EngineError::Profile(format!("profile not found: {profile_id}")))?;

        let display_name = name.unwrap_or_else(|| entry.name.clone());
        let mut profile = capture_profile_from_graph(&self.graph, profile_id, &display_name);
        profile.effect_state = store
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;

        if let Ok(existing) = profile_store.load_profile(&entry) {
            profile.created = existing.created;
        }
        update_profile_timestamp(&mut profile);

        profile_store
            .save_profile(&entry, &profile)
            .map_err(|error| EngineError::Profile(error.to_string()))?;

        if display_name != entry.name {
            let updated_entry = ProfileIndexEntry {
                id: entry.id,
                name: display_name.clone(),
                file: entry.file,
            };
            store
                .add_profile_to_index(updated_entry)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        profile.name = display_name;
        Ok(profile)
    }

    pub fn save_profile_as(
        &mut self,
        profile_id: &str,
        name: &str,
    ) -> Result<Profile, EngineError> {
        let store = ConfigStore::new();
        let profile_store = ProfileStore::new(store.config_dir().clone());
        let mut profile = capture_profile_from_graph(&self.graph, profile_id, name);
        profile.effect_state = store
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let entry = profile_store
            .save_profile_as(profile_id, name, &profile)
            .map_err(|error| EngineError::Profile(error.to_string()))?;
        store
            .add_profile_to_index(entry)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(profile)
    }

    pub fn import_profile(&self, source_path: &str) -> Result<ProfileIndexEntry, EngineError> {
        let store = ConfigStore::new();
        let profile_store = ProfileStore::new(store.config_dir().clone());
        let entry = profile_store
            .import_profile_file(Path::new(source_path))
            .map_err(|error| EngineError::Profile(error.to_string()))?;
        store
            .add_profile_to_index(entry.clone())
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(entry)
    }

    pub fn import_profile_archive(&self, source_path: &str) -> Result<ProfileIndexEntry, EngineError> {
        let store = ConfigStore::new();
        let profiles_dir = store.config_dir().join("profiles");
        let entry = import_profile_archive(Path::new(source_path), &profiles_dir)
            .map_err(|error| EngineError::Profile(error.to_string()))?;
        store
            .add_profile_to_index(entry.clone())
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(entry)
    }

    pub fn export_profile(&self, profile_id: &str, destination: &str) -> Result<(), EngineError> {
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let entry = config
            .profile_index
            .iter()
            .find(|entry| entry.id == profile_id)
            .cloned()
            .ok_or_else(|| EngineError::Profile(format!("profile not found: {profile_id}")))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());
        profile_store
            .export_profile_archive(&entry, Path::new(destination))
            .map_err(|error| EngineError::Profile(error.to_string()))
    }

    pub fn swap_profile(&mut self, profile_id: &str) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        self.manual_overrides.clear();
        self.device_manual_overrides.clear();
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());
        let profile = profile_store
            .load_profile_by_id(profile_id, &config.profile_index)
            .map_err(|error| EngineError::Profile(error.to_string()))?;

        let snapshot = capture_routing_snapshot(&self.graph);

        if self.graph.data_source != "mock" {
            let restore_result = restore::restore_profile_virtual_devices(
                &self.virtual_registry,
                &profile,
            )
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
            if !restore_result.errors.is_empty() {
                let message = restore_result.errors.join("; ");
                self.last_error = Some(message.clone());
                return Ok(ApplyResult {
                    success: false,
                    message: Some(message),
                });
            }
            self.refresh_graph()?;
        }

        let routing_result = if self.graph.data_source == "mock" {
            apply_mock_profile(&mut self.graph, &profile)
        } else {
            apply_profile_routing(&self.graph, &profile)
                .map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = routing_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            let _ = if self.graph.data_source == "mock" {
                apply_mock_snapshot(&mut self.graph, &snapshot)
            } else {
                restore_routing_snapshot(&self.graph, &snapshot)
                    .map_err(|error| EngineError::Routing(error.to_string()))
            };
            if self.graph.data_source != "mock" {
                self.refresh_graph()?;
            }
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if self.graph.data_source != "mock" {
            if let Err(error) = apply_profile_volumes(&self.graph, &profile) {
                let message = error.to_string();
                self.last_error = Some(message.clone());
                let _ = restore_routing_snapshot(&self.graph, &snapshot);
                self.refresh_graph()?;
                return Ok(ApplyResult {
                    success: false,
                    message: Some(message),
                });
            }
        } else {
            apply_mock_profile_volumes(&mut self.graph, &profile);
        }

        if self.graph.data_source != "mock" {
            if let Ok(warnings) = apply_profile_effects(&self.graph, &profile) {
                if let Err(error) = store.replace_effect_chains(profile.effect_state.clone()) {
                    self.last_error = Some(error.to_string());
                }
                if let Some(warning) = warnings.into_iter().next() {
                    self.last_error = Some(warning);
                }
            }
        }

        store
            .set_active_profile(profile_id)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }
}
