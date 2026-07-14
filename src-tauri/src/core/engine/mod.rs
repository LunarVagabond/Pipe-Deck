mod effects_ops;
mod graph_sync;
mod mixer_ops;
mod mock;
mod passthrough_ops;
mod profile_ops;
mod routing_ops;
mod virtual_ops;

use crate::config::ConfigStore;
use crate::core::models::{
    PluginStatus, Rule, RuntimeGraph, SimulationResult,
};
use crate::core::recent_streams::RecentStreamCache;
use crate::core::restore;
use crate::core::rules;
use crate::core::stream_identity::StreamIdentityKey;
use crate::backend::AudioBackend;
use crate::pipewire::filter_chain;
use crate::plugins::PluginManager;
use crate::backend::linux::virtual_devices::VirtualDeviceRegistry;
use std::collections::{HashMap, HashSet};
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
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

pub struct CoreEngine {
    graph: RuntimeGraph,
    adapter: Box<dyn AudioBackend>,
    rollback_stack: Vec<crate::core::routing::RoutingSnapshot>,
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
            adapter: crate::backend::create_backend(),
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
        let _ = rules::ensure_rules_migrated();
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
        rules::simulate_rules(&self.graph, &self.recent_streams)
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
