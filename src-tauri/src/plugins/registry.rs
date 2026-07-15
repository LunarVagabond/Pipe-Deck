use crate::config::ConfigStore;
use crate::core::models::{
    PluginDiscoveryIssue, PluginEntry, PluginRuntimeStatus, PluginStatus, PluginUiPanel,
    RuntimeGraph,
};
use crate::plugins::audit;
use crate::plugins::host::PluginProcess;
use crate::plugins::manifest::{discover_in_dir, DiscoveredPlugin};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Consecutive `start_plugin` failures after which a plugin is disabled outright
/// (see #102) rather than kept in an endless retry loop.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const BASE_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

fn backoff_for(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(16);
    BASE_BACKOFF.saturating_mul(1u32 << exponent).min(MAX_BACKOFF)
}

/// Tracks a plugin's recent crash history so `start_plugin` can back off instead of
/// respawning a broken plugin in a tight loop (#102).
#[derive(Default)]
struct RestartState {
    consecutive_failures: u32,
    next_retry_at: Option<Instant>,
    disabled_reason: Option<String>,
}

pub struct PluginManager {
    discovered: Vec<DiscoveredPlugin>,
    running: HashMap<String, PluginProcess>,
    last_errors: HashMap<String, String>,
    discovery_errors: Vec<PluginDiscoveryIssue>,
    restart_state: HashMap<String, RestartState>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            discovered: Vec::new(),
            running: HashMap::new(),
            last_errors: HashMap::new(),
            discovery_errors: Vec::new(),
            restart_state: HashMap::new(),
        }
    }

    pub fn discover(&mut self) {
        let mut all = Vec::new();
        let mut issues = Vec::new();
        if let Some(bundled) = bundled_plugins_dir() {
            let (plugins, dir_issues) = discover_in_dir(&bundled);
            all.extend(plugins);
            issues.extend(dir_issues);
        }
        let (plugins, dir_issues) = discover_in_dir(&user_plugins_dir());
        all.extend(plugins);
        issues.extend(dir_issues);

        for issue in &issues {
            audit::log(&issue.path, "discover", "error", Some(&issue.message));
        }

        self.discovered = dedupe_by_id(all);
        self.discovery_errors = issues;
    }

    pub fn discovery_errors(&self) -> Vec<PluginDiscoveryIssue> {
        self.discovery_errors.clone()
    }

    /// Re-runs discovery and starts any newly-enabled plugins, without requiring a full
    /// app restart (see #100/#123). Also stops any running process whose plugin directory
    /// has disappeared since the last scan, so a removed plugin doesn't keep running as
    /// an orphaned subprocess just because it fell out of `self.discovered`.
    pub fn rescan(&mut self) -> Result<(), String> {
        self.discover();
        self.ensure_bundled_defaults();

        let discovered_ids: HashSet<String> =
            self.discovered.iter().map(|plugin| plugin.manifest.id.clone()).collect();
        let orphaned: Vec<String> = self
            .running
            .keys()
            .filter(|id| !discovered_ids.contains(*id))
            .cloned()
            .collect();
        for plugin_id in orphaned {
            self.stop_plugin(&plugin_id);
        }

        self.start_enabled()
    }

    pub fn ensure_bundled_defaults(&self) {
        let store = ConfigStore::new();
        let mut config = store.load_config().unwrap_or_else(|_| ConfigStore::default_config());
        let mut changed = false;
        for plugin in &self.discovered {
            if !plugin.manifest.bundled {
                continue;
            }
            let entry = config.plugins.entry(plugin.manifest.id.clone()).or_insert_with(|| {
                changed = true;
                PluginEntry {
                    enabled: true,
                    granted_capabilities: plugin.manifest.capabilities.clone(),
                    config: serde_json::json!({}),
                }
            });
            if entry.granted_capabilities.is_empty() && !plugin.manifest.capabilities.is_empty() {
                entry.granted_capabilities = plugin.manifest.capabilities.clone();
                changed = true;
            }
        }
        if changed {
            let _ = store.save_config(&config);
        }
    }

    pub fn start_enabled(&mut self) -> Result<(), String> {
        let store = ConfigStore::new();
        let config = store.load_config().unwrap_or_else(|_| ConfigStore::default_config());
        for plugin in self.discovered.clone() {
            let Some(entry) = config.plugins.get(&plugin.manifest.id) else {
                continue;
            };
            if entry.enabled {
                if let Err(error) = self.start_plugin(&plugin.manifest.id) {
                    eprintln!("failed to start plugin {}: {error}", plugin.manifest.id);
                }
            }
        }
        Ok(())
    }

    pub fn start_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        if self.running.contains_key(plugin_id) {
            return Ok(());
        }
        if let Some(state) = self.restart_state.get(plugin_id) {
            if let Some(reason) = &state.disabled_reason {
                return Err(reason.clone());
            }
            if let Some(next_retry_at) = state.next_retry_at {
                if Instant::now() < next_retry_at {
                    return Err(format!(
                        "plugin {plugin_id} is backing off after repeated crashes, retrying later"
                    ));
                }
            }
        }
        let Some(discovered) = self
            .discovered
            .iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .cloned()
        else {
            return Err(format!("plugin not found: {plugin_id}"));
        };

        let store = ConfigStore::new();
        let config = store.load_config().map_err(|error| error.to_string())?;
        let entry = config
            .plugins
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        let granted = entry.granted_capabilities;

        let mut process = PluginProcess::spawn(
            &discovered.entry_path,
            plugin_id,
            &discovered.root,
        )
        .map_err(|error| error.to_string())?;

        if let Err(error) = process.initialize(
            plugin_id,
            &granted,
            store.config_dir(),
        ) {
            // The stderr reader thread runs independently of the RPC round-trip; give it
            // a brief grace period to catch up before reading the tail, since a plugin
            // typically writes to stderr just before/around the failure we just observed.
            std::thread::sleep(std::time::Duration::from_millis(20));
            let mut message = error.to_string();
            if let Some(stderr_tail) = process.stderr_tail() {
                message = format!("{message}\nstderr: {stderr_tail}");
            }
            self.last_errors.insert(plugin_id.to_string(), message.clone());
            audit::log(plugin_id, "initialize", "error", Some(&message));
            let _ = process.child.kill();

            let state = self.restart_state.entry(plugin_id.to_string()).or_default();
            state.consecutive_failures += 1;
            state.next_retry_at = Some(Instant::now() + backoff_for(state.consecutive_failures));
            if state.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                let reason = format!(
                    "disabled after {} consecutive crashes",
                    state.consecutive_failures
                );
                state.disabled_reason = Some(reason.clone());
                audit::log(plugin_id, "crash-loop", "error", Some(&reason));
            }

            return Err(message);
        }

        self.last_errors.remove(plugin_id);
        self.restart_state.remove(plugin_id);
        self.running.insert(plugin_id.to_string(), process);
        Ok(())
    }

    pub fn stop_plugin(&mut self, plugin_id: &str) {
        if let Some(mut process) = self.running.remove(plugin_id) {
            process.shutdown(plugin_id);
        }
    }

    pub fn set_enabled(&mut self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        let store = ConfigStore::new();
        let mut config = store.load_config().map_err(|error| error.to_string())?;
        let entry = config
            .plugins
            .entry(plugin_id.to_string())
            .or_insert_with(PluginEntry::default);
        entry.enabled = enabled;
        store.save_config(&config).map_err(|error| error.to_string())?;

        if enabled {
            // A user explicitly re-enabling a plugin is a deliberate retry — give it an
            // immediate attempt regardless of any crash-loop backoff/disabled state (#102).
            self.restart_state.remove(plugin_id);
            self.start_plugin(plugin_id)?;
        } else {
            self.stop_plugin(plugin_id);
        }
        Ok(())
    }

    pub fn grant_capabilities(
        &mut self,
        plugin_id: &str,
        capabilities: Vec<String>,
    ) -> Result<(), String> {
        let store = ConfigStore::new();
        let mut config = store.load_config().map_err(|error| error.to_string())?;
        let enabled = {
            let entry = config
                .plugins
                .entry(plugin_id.to_string())
                .or_insert_with(PluginEntry::default);
            entry.granted_capabilities = capabilities;
            entry.enabled
        };
        store.save_config(&config).map_err(|error| error.to_string())?;

        if enabled {
            self.stop_plugin(plugin_id);
            self.start_plugin(plugin_id)?;
        }
        Ok(())
    }

    pub fn push_graph(&mut self, graph: &RuntimeGraph) {
        let store = ConfigStore::new();
        let config = store.load_config().unwrap_or_else(|_| ConfigStore::default_config());
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for plugin_id in ids {
            let granted = config
                .plugins
                .get(&plugin_id)
                .map(|entry| entry.granted_capabilities.clone())
                .unwrap_or_default();
            if let Some(process) = self.running.get_mut(&plugin_id) {
                if crate::plugins::capabilities::is_granted(&granted, crate::plugins::capabilities::GRAPH_READ)
                    && process.notify_graph_updated(graph).is_err()
                {
                    let mut message = "graph notification failed".to_string();
                    if let Some(stderr_tail) = process.stderr_tail() {
                        message = format!("{message}\nstderr: {stderr_tail}");
                    }
                    self.last_errors.insert(plugin_id.clone(), message);
                }
                // Drain unconditionally: incoming plugin->host notifications (e.g.
                // ui.panel.register) must be processed regardless of which capability
                // triggered this tick — a plugin without graph.read but with
                // ui.panel.register would otherwise never get its panel registered.
                process.drain_notifications(&plugin_id, &granted);
            }
        }
    }

    /// Pushes the active profile's metadata to every running plugin granted
    /// `profile.read` (see #124), mirroring `push_graph`'s shape/gating.
    pub fn push_profile(&mut self, profile_id: &str, profile_name: &str, updated: &str) {
        let store = ConfigStore::new();
        let config = store.load_config().unwrap_or_else(|_| ConfigStore::default_config());
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for plugin_id in ids {
            let granted = config
                .plugins
                .get(&plugin_id)
                .map(|entry| entry.granted_capabilities.clone())
                .unwrap_or_default();
            if !crate::plugins::capabilities::is_granted(&granted, crate::plugins::capabilities::PROFILE_READ) {
                continue;
            }
            if let Some(process) = self.running.get_mut(&plugin_id) {
                if process.notify_profile_updated(profile_id, profile_name, updated).is_err() {
                    let mut message = "profile notification failed".to_string();
                    if let Some(stderr_tail) = process.stderr_tail() {
                        message = format!("{message}\nstderr: {stderr_tail}");
                    }
                    self.last_errors.insert(plugin_id.clone(), message);
                }
            }
        }
    }

    pub fn list_status(&self) -> Vec<PluginStatus> {
        let store = ConfigStore::new();
        let config = store.load_config().unwrap_or_else(|_| ConfigStore::default_config());
        self.discovered
            .iter()
            .map(|discovered| {
                let id = discovered.manifest.id.clone();
                let entry = config.plugins.get(&id).cloned().unwrap_or_default();
                let running = self.running.contains_key(&id);
                let runtime_status = if self.last_errors.contains_key(&id) {
                    PluginRuntimeStatus::Error
                } else if running {
                    PluginRuntimeStatus::Running
                } else {
                    PluginRuntimeStatus::Stopped
                };
                let ui_panels = self
                    .running
                    .get(&id)
                    .map(|process| process.ui_panels.clone())
                    .unwrap_or_default();
                let disabled_reason = self
                    .restart_state
                    .get(&id)
                    .and_then(|state| state.disabled_reason.clone());
                PluginStatus {
                    id: id.clone(),
                    name: discovered.manifest.name.clone(),
                    version: discovered.manifest.version.clone(),
                    description: discovered.manifest.description.clone(),
                    bundled: discovered.manifest.bundled,
                    developer: discovered.manifest.developer.clone(),
                    repo: discovered.manifest.repo.clone(),
                    enabled: entry.enabled,
                    requested_capabilities: discovered.manifest.capabilities.clone(),
                    granted_capabilities: entry.granted_capabilities.clone(),
                    runtime_status,
                    last_error: self.last_errors.get(&id).cloned(),
                    disabled_reason,
                    ui_panels,
                }
            })
            .collect()
    }

    pub fn ui_panels(&self) -> Vec<(String, PluginUiPanel)> {
        let mut panels = Vec::new();
        for (plugin_id, process) in &self.running {
            for panel in &process.ui_panels {
                panels.push((plugin_id.clone(), panel.clone()));
            }
        }
        panels
    }

    /// Drains every running plugin's queued `effects.apply` requests (see PD-021). This
    /// only ever moves requests out of `PluginProcess`-local storage — it does not apply
    /// them. Applying happens in `CoreEngine::apply_queued_plugin_effect_requests`, the
    /// only thing in this codebase with a `set_device_effects`/`AudioBackend` reference.
    pub fn drain_effects_requests(&mut self) -> Vec<(String, crate::core::models::EffectsApplyRequest)> {
        let mut drained = Vec::new();
        for (plugin_id, process) in self.running.iter_mut() {
            while let Some(request) = process.effects_requests.pop_front() {
                drained.push((plugin_id.clone(), request));
            }
        }
        drained
    }

    pub fn routing_suggestions(&self) -> Vec<crate::core::models::RoutingSuggestion> {
        let mut suggestions: Vec<_> = self
            .running
            .values()
            .flat_map(|process| process.routing_suggestions.iter().cloned())
            .collect();
        suggestions.sort_by(|left, right| left.received_at.cmp(&right.received_at));
        suggestions
    }

    pub fn shutdown_all(&mut self) {
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for plugin_id in ids {
            self.stop_plugin(&plugin_id);
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

fn dedupe_by_id(plugins: Vec<DiscoveredPlugin>) -> Vec<DiscoveredPlugin> {
    let mut map: HashMap<String, DiscoveredPlugin> = HashMap::new();
    for plugin in plugins {
        map.insert(plugin.manifest.id.clone(), plugin);
    }
    let mut values: Vec<_> = map.into_values().collect();
    values.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    values
}

pub fn bundled_plugins_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PIPE_DECK_BUNDLED_PLUGINS") {
        return Some(PathBuf::from(dir));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join("../plugins");
    if candidate.exists() {
        return Some(candidate);
    }
    None
}

pub fn user_plugins_dir() -> PathBuf {
    ConfigStore::new().config_dir().join("plugins")
}
