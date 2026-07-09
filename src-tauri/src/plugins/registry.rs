use crate::config::ConfigStore;
use crate::core::models::{
    PluginEntry, PluginRuntimeStatus, PluginStatus, PluginUiPanel, RuntimeGraph,
};
use crate::plugins::audit;
use crate::plugins::host::PluginProcess;
use crate::plugins::manifest::{discover_in_dir, DiscoveredPlugin};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct PluginManager {
    discovered: Vec<DiscoveredPlugin>,
    running: HashMap<String, PluginProcess>,
    last_errors: HashMap<String, String>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            discovered: Vec::new(),
            running: HashMap::new(),
            last_errors: HashMap::new(),
        }
    }

    pub fn discover(&mut self) {
        let mut all = Vec::new();
        if let Some(bundled) = bundled_plugins_dir() {
            all.extend(discover_in_dir(&bundled));
        }
        all.extend(discover_in_dir(&user_plugins_dir()));
        self.discovered = dedupe_by_id(all);
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
            let message = error.to_string();
            self.last_errors.insert(plugin_id.to_string(), message.clone());
            audit::log(plugin_id, "initialize", "error", Some(&message));
            let _ = process.child.kill();
            return Err(message);
        }

        self.last_errors.remove(plugin_id);
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
                if crate::plugins::capabilities::is_granted(&granted, crate::plugins::capabilities::GRAPH_READ) {
                    if process.notify_graph_updated(graph).is_err() {
                        self.last_errors.insert(
                            plugin_id.clone(),
                            "graph notification failed".into(),
                        );
                    }
                    process.drain_notifications(&plugin_id, &granted);
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
                PluginStatus {
                    id: id.clone(),
                    name: discovered.manifest.name.clone(),
                    version: discovered.manifest.version.clone(),
                    description: discovered.manifest.description.clone(),
                    bundled: discovered.manifest.bundled,
                    enabled: entry.enabled,
                    requested_capabilities: discovered.manifest.capabilities.clone(),
                    granted_capabilities: entry.granted_capabilities.clone(),
                    runtime_status,
                    last_error: self.last_errors.get(&id).cloned(),
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
