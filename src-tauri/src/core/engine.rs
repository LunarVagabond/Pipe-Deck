use crate::config::profile_store::{import_profile_archive, ProfileStore};
use crate::config::ConfigStore;
use crate::core::models::{
    ApplyResult, DeviceDirection, DeviceRouteIntent, EffectChainConfig, PluginStatus, Profile,
    ProfileIndexEntry, RoutingDrift, RoutingIntent, RuntimeGraph, Rule, SimulationResult,
    VirtualDeviceResult,
};
use crate::core::effects::apply_profile_effects;
use crate::core::profile::{capture_profile_from_graph, update_profile_timestamp};
use crate::core::profile_drift::compare_profile_to_graph;
use crate::core::restore::{self, spec_from_create_result};
use crate::core::recent_streams::RecentStreamCache;
use crate::core::rule_engine::{self, ApplyRulesContext};
use crate::core::stream_identity::{stream_identity_key, StreamIdentityKey};
use crate::core::routing::{
    apply_device_route_intent, apply_profile_routing, apply_profile_volumes, apply_routing_intent,
    capture_routing_snapshot, restore_routing_snapshot, RoutingSnapshot,
};
use crate::plugins::PluginManager;
use crate::pipewire::adapter::PipeWireAdapter;
use crate::pipewire::filter_chain;
use crate::pipewire::pactl;
use crate::pipewire::virtual_devices::VirtualDeviceRegistry;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("pipewire adapter error: {0}")]
    Adapter(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("profile error: {0}")]
    Profile(String),
    #[error("routing error: {0}")]
    Routing(String),
}

pub struct CoreEngine {
    graph: RuntimeGraph,
    adapter: Box<dyn PipeWireAdapter>,
    rollback_stack: Vec<RoutingSnapshot>,
    virtual_registry: Arc<VirtualDeviceRegistry>,
    device_id_remap: HashMap<String, String>,
    last_error: Option<String>,
    manual_overrides: HashSet<StreamIdentityKey>,
    device_manual_overrides: HashSet<String>,
    cleared_stream_routes: HashSet<StreamIdentityKey>,
    cleared_device_routes: HashSet<String>,
    plugin_manager: Mutex<PluginManager>,
    recent_streams: RecentStreamCache,
    seen_stream_identities: HashSet<StreamIdentityKey>,
}

impl CoreEngine {
    pub fn new() -> Self {
        Self {
            graph: RuntimeGraph::default(),
            adapter: create_adapter(),
            rollback_stack: Vec::new(),
            virtual_registry: VirtualDeviceRegistry::new(),
            device_id_remap: HashMap::new(),
            last_error: None,
            manual_overrides: HashSet::new(),
            device_manual_overrides: HashSet::new(),
            cleared_stream_routes: HashSet::new(),
            cleared_device_routes: HashSet::new(),
            plugin_manager: Mutex::new(PluginManager::new()),
            recent_streams: RecentStreamCache::default(),
            seen_stream_identities: HashSet::new(),
        }
    }

    pub fn runtime_graph(&self) -> &RuntimeGraph {
        &self.graph
    }

    pub fn virtual_registry(&self) -> &Arc<VirtualDeviceRegistry> {
        &self.virtual_registry
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn can_undo_routing(&self) -> bool {
        !self.rollback_stack.is_empty()
    }

    pub fn clear_last_error(&mut self) {
        self.last_error = None;
    }

    pub async fn initialize(
        &mut self,
        app: &AppHandle,
        engine_ref: Arc<RwLock<CoreEngine>>,
    ) -> Result<(), EngineError> {
        ConfigStore::new()
            .ensure_layout()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let _ = rule_engine::ensure_rules_migrated();
        self.initialize_plugins();

        if std::env::var("PIPE_DECK_USE_MOCK").as_deref() != Ok("1") {
            let restore_result = restore::restore_session(&self.virtual_registry)
                .map_err(|error| EngineError::Adapter(error.to_string()))?;
            self.apply_restore_notice(&restore_result);
            let _ = filter_chain::cleanup_effects_conf_files();
        }

        self.refresh_graph()?;
        let config = ConfigStore::new()
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        if config.preferences.restore_on_startup {
            let _ = self.apply_desired_routing();
        }
        if self.graph.data_source != "mock" {
            let _ = self.restore_effect_chains();
        }
        self.emit_graph_update(app);

        let app_handle = app.clone();
        self.adapter
            .subscribe(Box::new(move |graph| {
                let app_handle = app_handle.clone();
                let engine_ref = engine_ref.clone();
                tauri::async_runtime::spawn(async move {
                    let mut engine = engine_ref.write().await;
                    engine.apply_graph_update(graph);
                    let _ = app_handle.emit("graph-updated", engine.runtime_graph().clone());
                });
            }))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(())
    }

    pub fn initialize_plugins(&mut self) {
        if let Ok(mut plugins) = self.plugin_manager.lock() {
            plugins.discover();
            plugins.ensure_bundled_defaults();
            if let Err(error) = plugins.start_enabled().map_err(|e| e) {
                eprintln!("plugin start warning: {error}");
            }
        }
    }

    pub fn refresh_graph(&mut self) -> Result<(), EngineError> {
        let _ = self.virtual_registry.discover_from_pactl();
        self.graph = self
            .adapter
            .fetch_graph()
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        merge_virtual_devices(
            &mut self.graph,
            &self.virtual_registry,
            &mut self.device_id_remap,
        );
        self.sync_live_graph();
        self.finalize_graph_snapshot();
        self.apply_rules_for_new_streams();
        if let Ok(mut plugins) = self.plugin_manager.lock() {
            plugins.push_graph(&self.graph);
        }
        Ok(())
    }

    pub fn apply_graph_update(&mut self, graph: RuntimeGraph) {
        let _ = self.virtual_registry.discover_from_pactl();
        self.graph = graph;
        merge_virtual_devices(
            &mut self.graph,
            &self.virtual_registry,
            &mut self.device_id_remap,
        );
        self.sync_live_graph();
        self.finalize_graph_snapshot();
        self.apply_rules_for_new_streams();
        if let Ok(mut plugins) = self.plugin_manager.lock() {
            plugins.push_graph(&self.graph);
        }
    }

    fn finalize_graph_snapshot(&mut self) {
        self.recent_streams.record_streams(&self.graph.streams);
        self.graph.recent_stream_identities = self.recent_streams.list(&self.graph.streams);
    }

    fn sync_live_graph(&mut self) {
        crate::pipewire::live::sync_live_routing_graph(&mut self.graph);
        crate::pipewire::live::apply_user_cleared_routes(
            &mut self.graph,
            &self.cleared_stream_routes,
            &self.cleared_device_routes,
        );
    }

    pub fn apply_desired_routing(&mut self) -> Result<(), EngineError> {
        self.manual_overrides.clear();
        self.device_manual_overrides.clear();
        self.cleared_stream_routes.clear();
        self.cleared_device_routes.clear();
        self.apply_routing_rules();
        Ok(())
    }

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

    fn apply_routing_rules(&mut self) {
        let config = ConfigStore::new()
            .load_config()
            .unwrap_or_else(|_| ConfigStore::default_config());

        rule_engine::reconcile_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );
        rule_engine::reconcile_device_manual_overrides(
            &self.graph,
            &mut self.device_manual_overrides,
            &config.routing_rules.device_rules,
        );

        rule_engine::detect_external_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );
        rule_engine::detect_external_device_manual_overrides(
            &self.graph,
            &mut self.device_manual_overrides,
            &config.routing_rules.device_rules,
        );

        let ctx = ApplyRulesContext {
            manual_overrides: &self.manual_overrides,
            device_manual_overrides: &self.device_manual_overrides,
            dry_run: false,
            mock_graph_only: self.graph.data_source == "mock",
            limit_to_identities: None,
        };
        crate::pipewire::live::apply_graph_routing(&mut self.graph, &ctx);
    }

    fn apply_rules_for_new_streams(&mut self) {
        let config = ConfigStore::new()
            .load_config()
            .unwrap_or_else(|_| ConfigStore::default_config());
        if !config.preferences.auto_apply_rules {
            return;
        }

        let mut new_identities = HashSet::new();
        for stream in &self.graph.streams {
            if stream.is_system {
                continue;
            }
            let key = stream_identity_key(stream);
            if !self.seen_stream_identities.contains(&key) {
                new_identities.insert(key);
            }
        }

        if new_identities.is_empty() {
            return;
        }

        rule_engine::reconcile_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );
        rule_engine::detect_external_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );

        let ctx = ApplyRulesContext {
            manual_overrides: &self.manual_overrides,
            device_manual_overrides: &self.device_manual_overrides,
            dry_run: false,
            mock_graph_only: self.graph.data_source == "mock",
            limit_to_identities: Some(&new_identities),
        };
        let _ = rule_engine::apply_routing_rules_with_explanations(&mut self.graph, &ctx);

        for key in new_identities {
            self.seen_stream_identities.insert(key);
        }
    }

    fn sync_manual_override_for_ids(&mut self, stream_id: &str, target_device_id: &str) {
        let config = ConfigStore::new()
            .load_config()
            .unwrap_or_else(|_| ConfigStore::default_config());
        let Some((stream, target_system_name)) = (|| {
            let stream = self
                .graph
                .streams
                .iter()
                .find(|stream| stream.id == stream_id)?
                .clone();
            let target_system_name = self
                .graph
                .devices
                .iter()
                .find(|device| device.id == target_device_id)?
                .system_name
                .clone();
            Some((stream, target_system_name))
        })() else {
            return;
        };

        let identity = crate::core::stream_identity::stream_identity_key(&stream);
        if rule_engine::should_track_manual_override(
            &stream,
            &target_system_name,
            &config.rules,
            &config.routing_rules.stream_rules,
        ) {
            self.manual_overrides.insert(identity);
        } else {
            self.manual_overrides.remove(&identity);
        }
    }

    fn resolve_device_id(&self, device_id: &str) -> String {
        self.device_id_remap
            .get(device_id)
            .cloned()
            .unwrap_or_else(|| device_id.to_string())
    }

    pub fn emit_graph_update(&self, app: &AppHandle) {
        let _ = app.emit("graph-updated", self.graph.clone());
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

    pub fn set_stream_target(
        &mut self,
        stream_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        if let Some(stream) = self.graph.streams.iter().find(|stream| stream.id == stream_id) {
            self.cleared_stream_routes
                .remove(&crate::core::stream_identity::stream_identity_key(stream));
        }
        let snapshot = capture_routing_snapshot(&self.graph);
        let resolved_target = self.resolve_device_id(target_device_id);
        let intent = RoutingIntent {
            stream_id: stream_id.to_string(),
            target_device_id: Some(resolved_target.clone()),
            target_device_ids: Vec::new(),
        };

        let apply_result = if self.graph.data_source == "mock" {
            apply_mock_routing(&mut self.graph, &intent)
        } else {
            apply_routing_intent(&self.graph, &intent)
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

        if let Some(stream) = self.graph.streams.iter().find(|s| s.id == intent.stream_id) {
            if let Some(target) = self
                .graph
                .devices
                .iter()
                .find(|device| device.id == resolved_target)
            {
                let _ = crate::core::routing_rules::save_stream_route_rule(stream, target);
            }
        }

        self.sync_manual_override_for_ids(&intent.stream_id, &resolved_target);

        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn set_stream_targets(
        &mut self,
        stream_id: &str,
        target_device_ids: &[String],
    ) -> Result<ApplyResult, EngineError> {
        let Some(primary) = target_device_ids.first() else {
            return Ok(ApplyResult {
                success: false,
                message: Some("at least one target is required".into()),
            });
        };
        self.set_stream_target(stream_id, primary)
    }

    pub fn list_plugins(&self) -> Vec<PluginStatus> {
        self.plugin_manager
            .lock()
            .map(|manager| manager.list_status())
            .unwrap_or_default()
    }

    pub fn set_plugin_enabled(&mut self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        self.plugin_manager
            .lock()
            .map_err(|_| "plugin manager lock poisoned".to_string())?
            .set_enabled(plugin_id, enabled)
    }

    pub fn grant_plugin_capabilities(
        &mut self,
        plugin_id: &str,
        capabilities: Vec<String>,
    ) -> Result<(), String> {
        self.plugin_manager
            .lock()
            .map_err(|_| "plugin manager lock poisoned".to_string())?
            .grant_capabilities(plugin_id, capabilities)
    }

    pub fn plugin_ui_panels(&self) -> Vec<(String, crate::core::models::PluginUiPanel)> {
        self.plugin_manager
            .lock()
            .map(|manager| manager.ui_panels())
            .unwrap_or_default()
    }

    pub fn shutdown_plugins(&mut self) {
        if let Ok(mut manager) = self.plugin_manager.lock() {
            manager.shutdown_all();
        }
    }

    pub fn set_device_route(
        &mut self,
        source_device_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        self.set_device_targets(source_device_id, &[target_device_id.to_string()])
    }

    pub fn set_device_targets(
        &mut self,
        source_device_id: &str,
        target_device_ids: &[String],
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let snapshot = capture_routing_snapshot(&self.graph);
        let resolved_targets: Vec<String> = target_device_ids
            .iter()
            .map(|id| self.resolve_device_id(id))
            .collect();
        let intent = DeviceRouteIntent {
            source_device_id: self.resolve_device_id(source_device_id),
            target_device_id: resolved_targets.first().cloned(),
            target_device_ids: resolved_targets.clone(),
        };

        let apply_result = if self.graph.data_source == "mock" {
            apply_mock_device_route(&mut self.graph, &intent)
        } else {
            apply_device_route_intent(&self.graph, &intent)
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

        if let Some(source) = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == intent.source_device_id)
        {
            let targets: Vec<_> = resolved_targets
                .iter()
                .filter_map(|id| self.graph.devices.iter().find(|d| d.id == *id).cloned())
                .collect();
            if targets.is_empty() {
                let _ = crate::core::routing_rules::clear_device_route_rule(source);
                self.cleared_device_routes
                    .insert(intent.source_device_id.clone());
            } else {
                let _ = crate::core::routing_rules::save_device_route_rule(source, &targets);
                self.cleared_device_routes
                    .remove(&intent.source_device_id);
            }
        }

        if resolved_targets.is_empty() {
            if let Some(device) = self
                .graph
                .devices
                .iter_mut()
                .find(|device| device.id == intent.source_device_id)
            {
                device.current_target = None;
                device.current_targets.clear();
            }
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

    pub fn clear_stream_target(
        &mut self,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let snapshot = capture_routing_snapshot(&self.graph);

        let stream_identity = self
            .graph
            .streams
            .iter()
            .find(|stream| stream.id == stream_id)
            .map(crate::core::stream_identity::stream_identity_key);
        let Some(stream_identity) = stream_identity else {
            return Err(EngineError::Routing(format!("stream not found: {stream_id}")));
        };

        let apply_result: Result<(), EngineError> = if self.graph.data_source == "mock" {
            let Some(stream) = self
                .graph
                .streams
                .iter_mut()
                .find(|stream| stream.id == stream_id)
            else {
                return Err(EngineError::Routing(format!("stream not found: {stream_id}")));
            };
            stream.current_target = None;
            stream.current_targets.clear();
            Ok(())
        } else {
            crate::pipewire::pactl::clear_stream_target(
                &self.graph,
                stream_id,
                previous_target_device_id,
            )
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

        self.cleared_stream_routes.insert(stream_identity);

        if let Some(stream) = self.graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
            stream.current_target = None;
            stream.current_targets.clear();
        }

        if let Some(stream) = self.graph.streams.iter().find(|stream| stream.id == stream_id) {
            let _ = crate::core::routing_rules::clear_stream_route_rule(stream);
            self.manual_overrides
                .remove(&crate::core::stream_identity::stream_identity_key(stream));
        }

        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        } else {
            crate::pipewire::live::apply_user_cleared_routes(
                &mut self.graph,
                &self.cleared_stream_routes,
                &self.cleared_device_routes,
            );
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn undo_last_routing(&mut self) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let Some(snapshot) = self.rollback_stack.pop() else {
            return Ok(ApplyResult {
                success: false,
                message: Some("nothing to undo".into()),
            });
        };

        let restore_result = if self.graph.data_source == "mock" {
            apply_mock_snapshot(&mut self.graph, &snapshot)
        } else {
            restore_routing_snapshot(&self.graph, &snapshot)
                .map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = restore_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
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

    pub fn set_device_volume(&mut self, device_id: &str, percent: u8) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(device) = self.graph.devices.iter_mut().find(|device| device.id == device_id) {
                device.volume_percent = Some(percent.min(100));
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("device not found: {device_id}")));
        }

        pactl::set_device_volume(device_id, &self.graph, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_device_mute(&mut self, device_id: &str, muted: bool) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(device) = self.graph.devices.iter_mut().find(|device| device.id == device_id) {
                device.muted = Some(muted);
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("device not found: {device_id}")));
        }

        pactl::set_device_mute(device_id, &self.graph, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_stream_volume(&mut self, stream_id: &str, percent: u8) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(stream) = self.graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
                stream.volume_percent = Some(percent.min(100));
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("stream not found: {stream_id}")));
        }

        pactl::set_stream_volume(&self.graph, stream_id, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_stream_mute(&mut self, stream_id: &str, muted: bool) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(stream) = self.graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
                stream.muted = Some(muted);
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("stream not found: {stream_id}")));
        }

        pactl::set_stream_mute(&self.graph, stream_id, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn get_effect_chains(&self) -> Result<HashMap<String, EffectChainConfig>, EngineError> {
        ConfigStore::new()
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))
    }

    pub fn set_device_effects(
        &mut self,
        device_id: &str,
        config: EffectChainConfig,
    ) -> Result<ApplyResult, EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .ok_or_else(|| EngineError::Adapter(format!("device not found: {device_id}")))?;

        if !filter_chain::is_pipe_deck_device(&device.system_name) {
            return Err(EngineError::Adapter(
                "effects may only be applied to pipe-deck virtual devices".into(),
            ));
        }

        let store = ConfigStore::new();
        if config.is_active() {
            store
                .set_effect_chain(device_id, &config)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        } else {
            store
                .remove_effect_chain(device_id)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        if self.graph.data_source == "mock" {
            return Ok(ApplyResult {
                success: true,
                message: None,
            });
        }

        let chains = store
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let active = self.active_effect_chains(&chains);
        let deactivated = if config.is_active() {
            Vec::new()
        } else {
            vec![device.system_name.clone()]
        };

        filter_chain::sync_all_effects(&active, &deactivated)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn restore_effect_chains(&mut self) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            return Ok(());
        }

        let store = ConfigStore::new();
        let chains = store
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let active = self.active_effect_chains(&chains);
        let _ = filter_chain::sync_all_effects(&active, &[])
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(())
    }

    fn active_effect_chains(
        &self,
        chains: &std::collections::HashMap<String, EffectChainConfig>,
    ) -> Vec<(String, EffectChainConfig)> {
        chains
            .iter()
            .filter_map(|(device_id, config)| {
                if !config.is_active() {
                    return None;
                }
                let system_name = self
                    .graph
                    .devices
                    .iter()
                    .find(|device| device.id == *device_id)
                    .map(|device| device.system_name.clone())?;
                if !filter_chain::is_pipe_deck_device(&system_name) {
                    return None;
                }
                Some((system_name, config.clone()))
            })
            .collect()
    }

    pub fn create_virtual_output(&mut self, name: &str) -> Result<VirtualDeviceResult, EngineError> {
        self.create_virtual_output_with_mode(name, false)
    }

    pub fn create_virtual_multi_output(
        &mut self,
        name: &str,
    ) -> Result<VirtualDeviceResult, EngineError> {
        self.create_virtual_output_with_mode(name, true)
    }

    fn create_virtual_output_with_mode(
        &mut self,
        name: &str,
        multi: bool,
    ) -> Result<VirtualDeviceResult, EngineError> {
        if self.graph.data_source == "mock" {
            let slug = name.to_lowercase().replace(' ', "-");
            let system_name = format!("pipe-deck-{slug}");
            self.graph.devices.push(crate::core::models::Device {
                id: format!("virtual-{slug}"),
                system_name: system_name.clone(),
                label: name.to_string(),
                kind: crate::core::models::DeviceKind::Virtual,
                direction: crate::core::models::DeviceDirection::Output,
                sink_mode: Some(if multi {
                    crate::core::models::SinkMode::Multi
                } else {
                    crate::core::models::SinkMode::Single
                }),
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
                current_targets: Vec::new(),
            });
            return Ok(VirtualDeviceResult {
                device_id: format!("virtual-{slug}"),
                system_name,
                label: name.to_string(),
                multi,
            });
        }

        let result = if multi {
            self.virtual_registry
                .create_multi_output(name)
                .map_err(|error| EngineError::Adapter(error.to_string()))?
        } else {
            self.virtual_registry
                .create_output(name)
                .map_err(|error| EngineError::Adapter(error.to_string()))?
        };
        ConfigStore::new()
            .add_virtual_device(spec_from_create_result(
                &result.device_id,
                &result.system_name,
                &result.label,
                DeviceDirection::Output,
                multi,
            ))
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.refresh_graph()?;
        Ok(result)
    }

    pub fn create_virtual_input(&mut self, name: &str) -> Result<VirtualDeviceResult, EngineError> {
        if self.graph.data_source == "mock" {
            let slug = name.to_lowercase().replace(' ', "-");
            let system_name = format!("pipe-deck-{slug}");
            self.graph.devices.push(crate::core::models::Device {
                id: format!("virtual-{slug}"),
                system_name: system_name.clone(),
                label: name.to_string(),
                kind: crate::core::models::DeviceKind::Virtual,
                direction: crate::core::models::DeviceDirection::Input,
                sink_mode: None,
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
                current_targets: Vec::new(),
            });
            return Ok(VirtualDeviceResult {
                device_id: format!("virtual-{slug}"),
                system_name,
                label: name.to_string(),
                multi: false,
            });
        }

        let result = self
            .virtual_registry
            .create_input(name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        ConfigStore::new()
            .add_virtual_device(spec_from_create_result(
                &result.device_id,
                &result.system_name,
                &result.label,
                DeviceDirection::Input,
                false,
            ))
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.refresh_graph()?;
        Ok(result)
    }

    pub fn remove_virtual_device(&mut self, system_name: &str) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            self.graph
                .devices
                .retain(|device| device.system_name != system_name);
            return Ok(());
        }

        self.virtual_registry
            .remove_device(system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        ConfigStore::new()
            .remove_virtual_device(system_name)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn list_rules(&self) -> Result<Vec<Rule>, EngineError> {
        ConfigStore::new()
            .list_rules()
            .map_err(|error| EngineError::Config(error.to_string()))
    }

    pub fn save_rule(&self, rule: Rule) -> Result<(), EngineError> {
        ConfigStore::new()
            .save_rule(rule)
            .map_err(|error| EngineError::Config(error.to_string()))
    }

    pub fn delete_rule(&self, rule_id: &str) -> Result<(), EngineError> {
        ConfigStore::new()
            .delete_rule(rule_id)
            .map_err(|error| EngineError::Config(error.to_string()))
    }

    pub fn toggle_rule(&self, rule_id: &str, enabled: bool) -> Result<(), EngineError> {
        ConfigStore::new()
            .toggle_rule(rule_id, enabled)
            .map_err(|error| EngineError::Config(error.to_string()))
    }

    pub fn simulate_rules(&self) -> Vec<SimulationResult> {
        rule_engine::simulate_rules(&self.graph, &self.recent_streams)
    }

    fn apply_restore_notice(&mut self, result: &crate::core::models::RestoreResult) {
        let mut parts = Vec::new();
        if !result.created.is_empty() {
            parts.push(format!("Restored {} virtual device(s)", result.created.len()));
        }
        for warning in &result.warnings {
            parts.push(warning.clone());
        }
        for error in &result.errors {
            parts.push(format!("Restore error: {error}"));
        }
        if !parts.is_empty() {
            self.graph.notice = Some(parts.join(". "));
        }
    }
}

fn merge_virtual_devices(
    graph: &mut RuntimeGraph,
    registry: &VirtualDeviceRegistry,
    device_id_remap: &mut HashMap<String, String>,
) {
    let multi_by_name: HashMap<String, bool> = ConfigStore::new()
        .virtual_devices()
        .into_iter()
        .map(|spec| (format!("pipe-deck-{}", spec.slug), spec.multi))
        .collect();

    let mut id_remap = HashMap::new();

    for entry in registry.list_devices() {
        let sink_mode = if entry.direction == crate::core::models::DeviceDirection::Output {
            let multi = multi_by_name
                .get(&entry.system_name)
                .copied()
                .unwrap_or(entry.multi);
            Some(if multi {
                crate::core::models::SinkMode::Multi
            } else {
                crate::core::models::SinkMode::Single
            })
        } else {
            None
        };

        if let Some(device) = graph
            .devices
            .iter_mut()
            .find(|device| device.system_name == entry.system_name)
        {
            if device.id != entry.device_id {
                id_remap.insert(device.id.clone(), entry.device_id.clone());
            }
            device.id = entry.device_id.clone();
            device.label = entry.label.clone();
            device.kind = crate::core::models::DeviceKind::Virtual;
            device.direction = entry.direction.clone();
            device.sink_mode = sink_mode;
            if device.volume_percent.is_none() {
                device.volume_percent = Some(100);
            }
            if device.muted.is_none() {
                device.muted = Some(false);
            }
        } else {
            let mut device = entry.to_device();
            device.sink_mode = sink_mode;
            graph.devices.push(device);
        }
    }

    crate::pipewire::live::apply_device_aliases(&mut graph.devices);
    crate::pipewire::live::apply_device_levels(&mut graph.devices);

    for (old_id, new_id) in id_remap {
        device_id_remap.insert(old_id.clone(), new_id.clone());

        for stream in &mut graph.streams {
            if stream.current_target.as_deref() == Some(old_id.as_str()) {
                stream.current_target = Some(new_id.clone());
            }
        }

        for device in &mut graph.devices {
            if device.current_target.as_deref() == Some(old_id.as_str()) {
                device.current_target = Some(new_id.clone());
            }
        }

        for link in &mut graph.links {
            if link.source_id == old_id {
                link.source_id = new_id.clone();
            }
            if link.target_id == old_id {
                link.target_id = new_id.clone();
            }
        }
    }

    let mut seen_links = HashSet::new();
    graph.links.retain(|link| seen_links.insert((link.source_id.clone(), link.target_id.clone())));
}

fn apply_mock_routing(
    graph: &mut RuntimeGraph,
    intent: &RoutingIntent,
) -> Result<(), EngineError> {
    let target_id = intent
        .target_device_id
        .as_ref()
        .or_else(|| intent.target_device_ids.first())
        .ok_or_else(|| EngineError::Routing("routing intent has no target".into()))?;
    let stream = graph
        .streams
        .iter_mut()
        .find(|stream| stream.id == intent.stream_id)
        .ok_or_else(|| EngineError::Routing(format!("stream not found: {}", intent.stream_id)))?;
    if !graph.devices.iter().any(|device| device.id == *target_id) {
        return Err(EngineError::Routing(format!(
            "target device not found: {target_id}"
        )));
    }
    stream.current_target = Some(target_id.clone());
    stream.current_targets.clear();
    Ok(())
}

fn apply_mock_snapshot(
    graph: &mut RuntimeGraph,
    snapshot: &RoutingSnapshot,
) -> Result<(), EngineError> {
    for stream in &mut graph.streams {
        stream.current_target = None;
        stream.current_targets.clear();
    }
    for device in &mut graph.devices {
        device.current_target = None;
        device.current_targets.clear();
    }
    for intent in &snapshot.stream_intents {
        apply_mock_routing(graph, intent)?;
    }
    for intent in &snapshot.device_intents {
        apply_mock_device_route(graph, intent)?;
    }
    Ok(())
}

fn apply_mock_device_route(
    graph: &mut RuntimeGraph,
    intent: &DeviceRouteIntent,
) -> Result<(), EngineError> {
    let targets = intent.target_ids();
    if !graph
        .devices
        .iter()
        .any(|device| device.id == intent.source_device_id)
    {
        return Err(EngineError::Routing(format!(
            "source device not found: {}",
            intent.source_device_id
        )));
    }
    for target_id in &targets {
        if !graph.devices.iter().any(|device| device.id == *target_id) {
            return Err(EngineError::Routing(format!(
                "target device not found: {target_id}"
            )));
        }
    }

    let device = graph
        .devices
        .iter_mut()
        .find(|device| device.id == intent.source_device_id)
        .expect("source device exists");
    device.current_targets = targets.clone();
    device.current_target = targets.first().cloned();
    Ok(())
}

fn apply_mock_profile(graph: &mut RuntimeGraph, profile: &Profile) -> Result<(), EngineError> {
    for stream in &mut graph.streams {
        stream.current_target = None;
        stream.current_targets.clear();
    }
    for intent in &profile.routing_intents {
        apply_mock_routing(graph, intent)?;
    }
    Ok(())
}

fn apply_mock_profile_volumes(graph: &mut RuntimeGraph, profile: &Profile) {
    for (device_id, state) in &profile.volume_state {
        if let Some(device) = graph.devices.iter_mut().find(|device| device.id == *device_id) {
            device.volume_percent = Some(state.volume_percent);
            device.muted = Some(state.muted);
        }
    }
}

fn create_adapter() -> Box<dyn PipeWireAdapter> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Box::new(crate::pipewire::mock::MockPipeWireAdapter::new());
    }

    match crate::pipewire::live::LivePipeWireAdapter::new() {
        Ok(adapter) => Box::new(adapter),
        Err(error) => {
            eprintln!("PipeWire enumeration unavailable: {error}");
            Box::new(EmptyPipeWireAdapter {
                notice: format!("PipeWire unavailable: {error}"),
            })
        }
    }
}

struct EmptyPipeWireAdapter {
    notice: String,
}

impl PipeWireAdapter for EmptyPipeWireAdapter {
    fn fetch_graph(&self) -> Result<RuntimeGraph, crate::pipewire::adapter::AdapterError> {
        Ok(RuntimeGraph {
            notice: Some(self.notice.clone()),
            ..RuntimeGraph::default()
        })
    }

    fn subscribe(
        &self,
        _listener: crate::pipewire::adapter::GraphListener,
    ) -> Result<(), crate::pipewire::adapter::AdapterError> {
        Ok(())
    }
}
