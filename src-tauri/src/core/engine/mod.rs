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
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
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
    device_id_remap: HashMap<String, String>,
    last_error: Option<String>,
    manual_overrides: HashSet<StreamIdentityKey>,
    device_manual_overrides: HashSet<String>,
    cleared_stream_routes: HashSet<StreamIdentityKey>,
    cleared_device_routes: HashSet<String>,
    plugin_manager: Mutex<PluginManager>,
    recent_streams: RecentStreamCache,
    /// Stream instance ids (`Stream.id`, the PipeWire node id) already
    /// considered for auto-apply this session. Deliberately keyed on the
    /// per-instance id rather than `StreamIdentityKey` (app_name/executable/
    /// media_name): apps like Firefox tear down and recreate their stream
    /// node per tab while reporting identical identity metadata across
    /// tabs, so an identity-keyed set would permanently mark all future
    /// Firefox streams "already seen" after the first one (issue #277/#116).
    /// Pruned each refresh to the currently-live id set in
    /// `apply_rules_for_new_streams`, and cleared entirely on rule
    /// create/edit/delete/toggle so already-live streams get re-evaluated
    /// against the changed rule set without requiring a manual "Apply rules".
    seen_stream_ids: HashSet<String>,
    /// system_name -> was its native effect chain live as of the last graph
    /// refresh (issue #206). Native-effects-only: the restart-based path's
    /// liveness never flips independently of a GUI-initiated call, so it
    /// never trips the reappeared-since-last-refresh check below.
    effect_chain_liveness: HashMap<String, bool>,
    /// device_id -> last-known downstream targets while its effect chain
    /// was live, so a chain that reappears after a daemon crash-recovery
    /// reload (`daemon::reconcile_live_effects_state`) can have its routing
    /// restored immediately instead of waiting for a user-triggered rules
    /// re-apply.
    effect_chain_last_targets: HashMap<String, Vec<String>>,
    /// Bumped by every command-driven `refresh_graph()` (never by the
    /// passive `pw-dump` monitor's `apply_graph_update`). A plain field
    /// would be enough for command paths, which already hold `&mut self`
    /// under the engine's write lock, but the monitor thread in
    /// `backend::linux::live::run_pw_dump_monitor` samples PipeWire
    /// (`enumerate_pipewire()`) *before* it ever touches the engine lock —
    /// it needs to read "what generation was authoritative right as I
    /// finished sampling" without awaiting that lock, which only a shared
    /// atomic handle (see `graph_generation_handle`) allows. Exists to fix
    /// issue #229's stale-graph-view symptom: the monitor can sample
    /// mid-restart and only get to apply/emit its (stale) snapshot after a
    /// command's own final, correct emit — comparing generations lets that
    /// stale snapshot be dropped instead of overwriting the correct one.
    graph_generation: Arc<AtomicU64>,
}

impl CoreEngine {
    pub fn new() -> Self {
        Self {
            graph: RuntimeGraph::default(),
            adapter: crate::backend::create_backend(),
            rollback_stack: Vec::new(),
            device_id_remap: HashMap::new(),
            last_error: None,
            manual_overrides: HashSet::new(),
            device_manual_overrides: HashSet::new(),
            cleared_stream_routes: HashSet::new(),
            cleared_device_routes: HashSet::new(),
            plugin_manager: Mutex::new(PluginManager::new()),
            recent_streams: RecentStreamCache::default(),
            seen_stream_ids: HashSet::new(),
            effect_chain_liveness: HashMap::new(),
            effect_chain_last_targets: HashMap::new(),
            graph_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn runtime_graph(&self) -> &RuntimeGraph {
        &self.graph
    }

    /// A cloned handle to the command-driven graph-generation counter, for
    /// the `pw-dump` monitor subscription set up in `initialize()` to read
    /// without needing the engine's write lock. See the field doc comment.
    pub fn graph_generation_handle(&self) -> Arc<AtomicU64> {
        self.graph_generation.clone()
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
            let restore_result = restore::restore_session(self.adapter.as_ref())
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
        self.emit_graph_update(app);

        // Reapplying previously-live effect chains does a native-effects
        // daemon round trip per device (`is_effect_chain_loaded`), which can
        // block for several seconds if the daemon — just spawned by
        // `ensure_ephemeral_daemon` moments earlier — hasn't opened its IPC
        // socket for `accept()` yet. Running it inline here, before
        // returning, held the engine write lock for that whole stall: every
        // other command waiting on the same lock (starting with the
        // frontend's very first `get_runtime_graph` call on app boot) queued
        // behind it, which read as a multi-second blank window on cold
        // launch. Spawning it separately re-acquires the lock only once the
        // daemon round trip actually needs it, so the graph handoff above
        // isn't held hostage by it.
        if self.graph.data_source != "mock" {
            let engine_for_effects = engine_ref.clone();
            let app_for_effects = app.clone();
            tauri::async_runtime::spawn(async move {
                let mut engine = engine_for_effects.write().await;
                if engine.restore_effect_chains().is_ok() {
                    engine.emit_graph_update(&app_for_effects);
                }
            });
        }

        let app_handle = app.clone();
        let generation_handle = self.graph_generation_handle();
        self.adapter
            .subscribe(Box::new(move |graph| {
                // Snapshot the generation as of right after this graph was
                // sampled from PipeWire (see the `graph_generation` field
                // doc) — if a command-driven `refresh_graph()` completes and
                // bumps it before this update gets to apply below, this
                // snapshot is stale (it may reflect a mid-restart transient
                // state) and must be dropped rather than overwrite the
                // command's already-correct, already-emitted state.
                let observed_generation = generation_handle.load(Ordering::SeqCst);
                let app_handle = app_handle.clone();
                let engine_ref = engine_ref.clone();
                let generation_handle = generation_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let mut engine = engine_ref.write().await;
                    if generation_handle.load(Ordering::SeqCst) != observed_generation {
                        return;
                    }
                    engine.apply_graph_update(graph);
                    let _ = app_handle.emit("graph-updated", engine.runtime_graph().clone());
                });
            }))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(())
    }

    pub fn initialize_plugins(&mut self) {
        {
            let Ok(mut plugins) = self.plugin_manager.lock() else {
                return;
            };
            plugins.discover();
            plugins.ensure_bundled_defaults();
            if let Err(error) = plugins.start_enabled() {
                eprintln!("plugin start warning: {error}");
            }
        }
        self.push_active_profile_to_plugins();
    }

    pub fn platform_audio_version(&self) -> Option<String> {
        self.adapter.platform_audio_version()
    }

    /// Unconditionally unloads every live Pipe Deck virtual device module —
    /// see `restore::remove_all_virtual_devices` for why this is distinct
    /// from the config-diffed orphan cleanup `initialize()` runs.
    pub fn remove_all_virtual_devices(&self) -> (Vec<String>, Vec<String>) {
        restore::remove_all_virtual_devices(self.adapter.as_ref())
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

    pub fn plugin_discovery_errors(&self) -> Vec<crate::core::models::PluginDiscoveryIssue> {
        self.plugin_manager
            .lock()
            .map(|manager| manager.discovery_errors())
            .unwrap_or_default()
    }

    pub fn rescan_plugins(&mut self) -> Result<(), String> {
        {
            let mut plugins = self
                .plugin_manager
                .lock()
                .map_err(|_| "plugin manager lock poisoned".to_string())?;
            plugins.rescan()?;
        }
        self.push_active_profile_to_plugins();
        Ok(())
    }

    pub fn plugin_routing_suggestions(&self) -> Vec<crate::core::models::RoutingSuggestion> {
        self.plugin_manager
            .lock()
            .map(|manager| manager.routing_suggestions())
            .unwrap_or_default()
    }

    /// Applies any `effects.apply` requests plugins queued since the last tick, via the
    /// same `set_device_effects` path first-party UI already uses (device/safety checks
    /// included) — see PD-021. Called once per graph refresh; never called from inside
    /// the plugin host itself, which has no reference to `self`/`AudioBackend`.
    pub fn apply_queued_plugin_effect_requests(&mut self) {
        let requests: Vec<(String, crate::core::models::EffectsApplyRequest)> = {
            let Ok(mut plugins) = self.plugin_manager.lock() else {
                return;
            };
            plugins.drain_effects_requests()
        };
        for (plugin_id, request) in requests {
            match self.set_device_effects(&request.device_id, request.config) {
                Ok(result) if result.success => {
                    crate::plugins::audit::log(&plugin_id, "effects.apply", "ok", None);
                }
                Ok(result) => {
                    crate::plugins::audit::log(
                        &plugin_id,
                        "effects.apply",
                        "error",
                        result.message.as_deref(),
                    );
                }
                Err(error) => {
                    let message = error.to_string();
                    crate::plugins::audit::log(&plugin_id, "effects.apply", "error", Some(&message));
                }
            }
        }
    }

    /// Pushes the currently active profile's metadata to every running plugin granted
    /// `profile.read`. Called after a profile swap and whenever plugins (re)start, so a
    /// freshly-started plugin doesn't have to wait for the next swap to learn what's active.
    pub fn push_active_profile_to_plugins(&mut self) {
        let Some(profile_id) = ConfigStore::new()
            .load_config()
            .ok()
            .and_then(|config| config.active_profile)
        else {
            return;
        };
        let Ok(profile) = self.get_profile(&profile_id) else {
            return;
        };
        if let Ok(mut plugins) = self.plugin_manager.lock() {
            plugins.push_profile(&profile.id, &profile.name, &profile.updated);
        }
    }

    pub fn plugin_capability_metadata(&self) -> Vec<crate::core::models::CapabilityInfo> {
        crate::plugins::capabilities::all_metadata()
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

    pub fn save_rule(&mut self, rule: Rule) -> Result<(), EngineError> {
        ConfigStore::new()
            .save_rule(rule)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.seen_stream_ids.clear();
        Ok(())
    }

    pub fn delete_rule(&mut self, rule_id: &str) -> Result<(), EngineError> {
        ConfigStore::new()
            .delete_rule(rule_id)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.seen_stream_ids.clear();
        Ok(())
    }

    pub fn toggle_rule(&mut self, rule_id: &str, enabled: bool) -> Result<(), EngineError> {
        ConfigStore::new()
            .toggle_rule(rule_id, enabled)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.seen_stream_ids.clear();
        Ok(())
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
