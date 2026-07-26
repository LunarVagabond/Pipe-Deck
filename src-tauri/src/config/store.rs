use crate::core::models::{
    AppConfig, DeviceAliasEntry, EffectChainConfig, Preferences, ProcessingNodePortSpec,
    ProcessingNodeSpec, ProcessingNodeSpecKind, ProfileIndexEntry, Rule, RoutingRulesConfig,
    VirtualDeviceSpec,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Read(String),
    #[error("failed to write config: {0}")]
    Write(String),
}

pub struct ConfigStore {
    config_dir: PathBuf,
}

const EFFECTS_PLUGIN_ID: &str = "pipe-deck-effects";

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigStore {
    pub fn new() -> Self {
        let config_dir = Self::default_config_dir();
        Self { config_dir }
    }

    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.yaml")
    }

    fn default_config_dir() -> PathBuf {
        if let Ok(path) = std::env::var("PIPE_DECK_CONFIG_DIR") {
            return PathBuf::from(path);
        }

        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("pipe-deck");
        }

        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config/pipe-deck"))
            .unwrap_or_else(|_| PathBuf::from(".pipe-deck"))
    }

    pub fn default_config() -> AppConfig {
        AppConfig {
            version: 1,
            active_profile: Some("default".into()),
            profile_index: vec![ProfileIndexEntry {
                id: "default".into(),
                name: "Default".into(),
                file: "profiles/default.yaml".into(),
            }],
            devices: HashMap::new(),
            preferences: Preferences::default(),
            routing_rules: RoutingRulesConfig::default(),
            rules: Vec::new(),
            virtual_devices: Vec::new(),
            processing_nodes: Vec::new(),
            plugins: HashMap::new(),
        }
    }

    pub fn routing_rules(&self) -> RoutingRulesConfig {
        self.load_config()
            .map(|config| config.routing_rules)
            .unwrap_or_default()
    }

    pub fn save_routing_rules(&self, rules: &RoutingRulesConfig) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.routing_rules = rules.clone();
        self.save_config(&config)
    }

    pub fn load_config(&self) -> Result<AppConfig, ConfigError> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(Self::default_config());
        }

        let contents = fs::read_to_string(&path)
            .map_err(|error| ConfigError::Read(format!("{path:?}: {error}")))?;
        let mut config: AppConfig = serde_yaml::from_str(&contents)
            .map_err(|error| ConfigError::Read(format!("{path:?}: {error}")))?;
        if migrate_mix_sources_to_mixer_nodes(&mut config) {
            // Best-effort: a failed write here just means this same
            // migration re-runs (harmlessly, idempotently) on the next load
            // instead of being durable yet.
            let _ = self.save_config(&config);
        }
        Ok(config)
    }

    pub fn processing_nodes(&self) -> Vec<ProcessingNodeSpec> {
        self.load_config()
            .map(|config| config.processing_nodes)
            .unwrap_or_default()
    }

    pub fn save_processing_nodes(&self, nodes: &[ProcessingNodeSpec]) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.processing_nodes = nodes.to_vec();
        self.save_config(&config)
    }

    pub fn add_processing_node(&self, spec: ProcessingNodeSpec) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.processing_nodes.push(spec);
        self.save_config(&config)
    }

    pub fn remove_processing_node(&self, id: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.processing_nodes.retain(|node| node.id != id);
        self.save_config(&config)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_processing_node_eq(
        &self,
        node_id: &str,
        eq_sub: i32,
        eq_bass: i32,
        eq_mid: i32,
        eq_treble: i32,
        eq_air: i32,
        output_gain: i32,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(node) = config.processing_nodes.iter_mut().find(|node| node.id == node_id) else {
            return Ok(());
        };
        if let ProcessingNodeSpecKind::Eq5Band { .. } = &node.kind {
            node.kind = ProcessingNodeSpecKind::Eq5Band { eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain };
        }
        self.save_config(&config)
    }

    pub fn set_processing_node_volume(&self, node_id: &str, volume_percent: u8, muted: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(node) = config.processing_nodes.iter_mut().find(|node| node.id == node_id) else {
            return Ok(());
        };
        if let ProcessingNodeSpecKind::FanOut { .. } = &node.kind {
            node.kind = ProcessingNodeSpecKind::FanOut { volume_percent, muted };
        }
        self.save_config(&config)
    }

    pub fn set_processing_node_bypassed(&self, node_id: &str, bypassed: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(node) = config.processing_nodes.iter_mut().find(|node| node.id == node_id) else {
            return Ok(());
        };
        node.bypassed = bypassed;
        self.save_config(&config)
    }

    /// Persists `peer_system_name` at `port_index` on the given `direction`
    /// for the named node — extending the port list if `port_index` is one
    /// past the current end (a freshly grown port), overwriting in place
    /// otherwise. A no-op (not an error) if the node itself was removed out
    /// from under a still-in-flight connect call.
    pub fn upsert_processing_node_port(
        &self,
        node_id: &str,
        direction: crate::core::models::PortDirection,
        port_index: u32,
        peer_system_name: &str,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(node) = config.processing_nodes.iter_mut().find(|node| node.id == node_id) else {
            return Ok(());
        };
        let index = port_index as usize;
        match direction {
            crate::core::models::PortDirection::Input => {
                let entry = ProcessingNodePortSpec {
                    source_system_name: peer_system_name.to_string(),
                    gain_percent: 100,
                    muted: false,
                };
                if index < node.input_sources.len() {
                    node.input_sources[index] = entry;
                } else {
                    node.input_sources.push(entry);
                }
            }
            crate::core::models::PortDirection::Output => {
                if index < node.output_targets.len() {
                    node.output_targets[index] = peer_system_name.to_string();
                } else {
                    node.output_targets.push(peer_system_name.to_string());
                }
            }
        }
        self.save_config(&config)
    }

    pub fn set_processing_node_input_gain(
        &self,
        node_id: &str,
        port_index: u32,
        gain_percent: u8,
        muted: bool,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(node) = config.processing_nodes.iter_mut().find(|node| node.id == node_id) else {
            return Ok(());
        };
        if let Some(entry) = node.input_sources.get_mut(port_index as usize) {
            entry.gain_percent = gain_percent;
            entry.muted = muted;
        }
        self.save_config(&config)
    }

    pub fn remove_processing_node_port(
        &self,
        node_id: &str,
        direction: crate::core::models::PortDirection,
        port_index: u32,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(node) = config.processing_nodes.iter_mut().find(|node| node.id == node_id) else {
            return Ok(());
        };
        let index = port_index as usize;
        match direction {
            crate::core::models::PortDirection::Input => {
                if index < node.input_sources.len() {
                    node.input_sources.remove(index);
                }
            }
            crate::core::models::PortDirection::Output => {
                if index < node.output_targets.len() {
                    node.output_targets.remove(index);
                }
            }
        }
        self.save_config(&config)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.config_dir)
            .map_err(|error| ConfigError::Write(format!("{error}")))?;

        let contents = serde_yaml::to_string(config)
            .map_err(|error| ConfigError::Write(format!("{error}")))?;
        fs::write(self.config_path(), contents)
            .map_err(|error| ConfigError::Write(format!("{error}")))
    }

    pub fn device_aliases(&self) -> HashMap<String, String> {
        self.load_config()
            .map(|config| {
                config
                    .devices
                    .into_iter()
                    .map(|(system_name, entry)| (system_name, entry.alias))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_device_alias(&self, system_name: &str, alias: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.devices.insert(
            system_name.to_string(),
            DeviceAliasEntry {
                alias: alias.to_string(),
            },
        );
        if let Some(slug) = system_name
            .strip_prefix("pipe-deck-")
            .filter(|_| !system_name.starts_with("pipe-deck-feed-"))
        {
            if let Some(entry) = config.virtual_devices.iter_mut().find(|entry| entry.slug == slug)
            {
                entry.label = alias.to_string();
            }
        }
        self.save_config(&config)
    }

    pub fn set_show_system_streams(&self, show: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.show_system_streams = show;
        self.save_config(&config)
    }

    pub fn set_auto_apply_rules(&self, enabled: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.auto_apply_rules = enabled;
        self.save_config(&config)
    }

    pub fn set_sidebar_collapsed(&self, collapsed: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.sidebar_collapsed = collapsed;
        self.save_config(&config)
    }

    pub fn set_theme_mode(&self, mode: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.theme_mode = mode.to_string();
        self.save_config(&config)
    }

    pub fn set_dark_scheme(&self, id: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.dark_scheme = id.to_string();
        self.save_config(&config)
    }

    pub fn set_light_scheme(&self, id: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.light_scheme = id.to_string();
        self.save_config(&config)
    }

    pub fn set_notice_duration_ms(&self, ms: u32) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.notice_duration_ms = ms;
        self.save_config(&config)
    }

    pub fn virtual_devices(&self) -> Vec<VirtualDeviceSpec> {
        self.load_config()
            .map(|config| config.virtual_devices)
            .unwrap_or_default()
    }

    pub fn save_virtual_devices(&self, devices: &[VirtualDeviceSpec]) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.virtual_devices = devices.to_vec();
        self.save_config(&config)
    }

    pub fn add_virtual_device(&self, spec: VirtualDeviceSpec) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        if let Some(existing) = config
            .virtual_devices
            .iter_mut()
            .find(|entry| entry.id == spec.id || entry.slug == spec.slug)
        {
            *existing = spec;
        } else {
            config.virtual_devices.push(spec);
        }
        self.save_config(&config)
    }

    pub fn set_virtual_mic_mix_sources(
        &self,
        virtual_system_name: &str,
        mix_sources: &[crate::core::models::MixSourceSpec],
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let slug = virtual_system_name
            .strip_prefix("pipe-deck-")
            .unwrap_or(virtual_system_name);
        let Some(spec) = config
            .virtual_devices
            .iter_mut()
            .find(|entry| {
                entry.slug == slug || format!("pipe-deck-{}", entry.slug) == virtual_system_name
            })
        else {
            return Err(ConfigError::Read(format!(
                "virtual device not found: {virtual_system_name}"
            )));
        };
        spec.mix_sources = mix_sources.to_vec();
        self.save_config(&config)
    }

    /// Updates the persisted gain for one already-mixed source without
    /// touching the rest of the mix list.
    pub fn update_mix_source_volume(
        &self,
        virtual_system_name: &str,
        source_system_name: &str,
        percent: u8,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let slug = virtual_system_name
            .strip_prefix("pipe-deck-")
            .unwrap_or(virtual_system_name);
        let Some(spec) = config
            .virtual_devices
            .iter_mut()
            .find(|entry| {
                entry.slug == slug || format!("pipe-deck-{}", entry.slug) == virtual_system_name
            })
        else {
            return Err(ConfigError::Read(format!(
                "virtual device not found: {virtual_system_name}"
            )));
        };
        if let Some(source) = spec
            .mix_sources
            .iter_mut()
            .find(|source| source.system_name == source_system_name)
        {
            source.volume_percent = percent;
        }
        self.save_config(&config)
    }

    /// Updates the persisted mute state for one already-mixed source without
    /// touching the rest of the mix list.
    pub fn update_mix_source_mute(
        &self,
        virtual_system_name: &str,
        source_system_name: &str,
        muted: bool,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let slug = virtual_system_name
            .strip_prefix("pipe-deck-")
            .unwrap_or(virtual_system_name);
        let Some(spec) = config
            .virtual_devices
            .iter_mut()
            .find(|entry| {
                entry.slug == slug || format!("pipe-deck-{}", entry.slug) == virtual_system_name
            })
        else {
            return Err(ConfigError::Read(format!(
                "virtual device not found: {virtual_system_name}"
            )));
        };
        if let Some(source) = spec
            .mix_sources
            .iter_mut()
            .find(|source| source.system_name == source_system_name)
        {
            source.muted = muted;
        }
        self.save_config(&config)
    }

    pub fn remove_virtual_device(&self, id_or_system_name: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let slug = id_or_system_name
            .strip_prefix("pipe-deck-")
            .unwrap_or(id_or_system_name)
            .strip_prefix("virtual-")
            .unwrap_or(id_or_system_name);
        config.virtual_devices.retain(|entry| {
            entry.id != id_or_system_name
                && entry.slug != slug
                && entry.id != format!("virtual-{slug}")
                && format!("pipe-deck-{}", entry.slug) != id_or_system_name
        });
        self.save_config(&config)
    }

    pub fn set_restore_on_startup(&self, enabled: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.restore_on_startup = enabled;
        self.save_config(&config)
    }

    pub fn set_background_restore(&self, enabled: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.background_restore = enabled;
        self.save_config(&config)
    }

    pub fn preferences(&self) -> Preferences {
        self.load_config()
            .map(|config| config.preferences)
            .unwrap_or_default()
    }

    pub fn ensure_layout(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.config_dir)
            .map_err(|error| ConfigError::Write(format!("{error}")))?;
        fs::create_dir_all(self.config_dir.join("profiles"))
            .map_err(|error| ConfigError::Write(format!("{error}")))?;
        // Mirrors profiles/ — a directory the user can browse to and drop a plugin into
        // (see docs/developers/Plugins.md quick start), same idea as themes/ for custom color schemes.
        fs::create_dir_all(self.config_dir.join("plugins"))
            .map_err(|error| ConfigError::Write(format!("{error}")))?;

        let profile_store = crate::config::profile_store::ProfileStore::new(self.config_dir.clone());
        profile_store
            .ensure_default_profile()
            .map_err(|error| ConfigError::Write(error.to_string()))?;

        if !self.config_path().exists() {
            self.save_config(&Self::default_config())?;
        }

        Ok(())
    }

    pub fn add_profile_to_index(&self, entry: ProfileIndexEntry) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        if let Some(existing) = config.profile_index.iter_mut().find(|item| item.id == entry.id) {
            *existing = entry;
        } else {
            config.profile_index.push(entry);
        }
        self.save_config(&config)
    }

    pub fn set_active_profile(&self, profile_id: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.active_profile = Some(profile_id.to_string());
        self.save_config(&config)
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileIndexEntry>, ConfigError> {
        Ok(self.load_config()?.profile_index)
    }

    pub fn list_rules(&self) -> Result<Vec<Rule>, ConfigError> {
        Ok(self.load_config()?.rules)
    }

    pub fn save_rule(&self, rule: Rule) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        if let Some(existing) = config.rules.iter_mut().find(|item| item.id == rule.id) {
            *existing = rule;
        } else {
            config.rules.push(rule);
        }
        config.rules.sort_by(|left, right| right.priority.cmp(&left.priority));
        self.save_config(&config)
    }

    pub fn delete_rule(&self, rule_id: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.rules.retain(|rule| rule.id != rule_id);
        self.save_config(&config)
    }

    pub fn toggle_rule(&self, rule_id: &str, enabled: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let Some(rule) = config.rules.iter_mut().find(|rule| rule.id == rule_id) else {
            return Err(ConfigError::Read(format!("rule not found: {rule_id}")));
        };
        rule.enabled = enabled;
        self.save_config(&config)
    }

    pub fn effect_chains(&self) -> Result<HashMap<String, EffectChainConfig>, ConfigError> {
        let config = self.load_config()?;
        Ok(Self::parse_effect_chains(&config))
    }

    pub fn set_effect_chain(
        &self,
        device_id: &str,
        chain: &EffectChainConfig,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let mut chains = Self::parse_effect_chains(&config);
        chains.insert(device_id.to_string(), chain.clone());
        Self::write_effect_chains(&mut config, chains);
        self.save_config(&config)
    }

    pub fn remove_effect_chain(&self, device_id: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let mut chains = Self::parse_effect_chains(&config);
        chains.remove(device_id);
        Self::write_effect_chains(&mut config, chains);
        self.save_config(&config)
    }

    pub fn replace_effect_chains(
        &self,
        chains: HashMap<String, EffectChainConfig>,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        Self::write_effect_chains(&mut config, chains);
        self.save_config(&config)
    }

    fn parse_effect_chains(config: &AppConfig) -> HashMap<String, EffectChainConfig> {
        config
            .plugins
            .get(EFFECTS_PLUGIN_ID)
            .and_then(|entry| entry.config.get("chains"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    fn write_effect_chains(config: &mut AppConfig, chains: HashMap<String, EffectChainConfig>) {
        let plugin = config
            .plugins
            .entry(EFFECTS_PLUGIN_ID.to_string())
            .or_default();
        let mut plugin_config = if plugin.config.is_object() {
            plugin.config.as_object().cloned().unwrap_or_default()
        } else {
            serde_json::Map::new()
        };
        plugin_config.insert(
            "chains".into(),
            serde_json::to_value(chains).unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        );
        plugin.config = serde_json::Value::Object(plugin_config);
    }
}

/// PD-032: mic-mix is retired in favor of the generic Mixer processing-node
/// kind. Any virtual input device with `mix_sources` still configured the
/// old way, and no `Mixer` node already feeding it, gets an equivalent
/// `ProcessingNodeSpec::Mixer` synthesized here — same pattern as
/// `EffectChainConfig`'s legacy flat-`eq_*`-fields migration, except this
/// one also clears the source `mix_sources` it migrated from, so the
/// rewrite is durable once `load_config` saves it back rather than
/// re-running against stale data forever. Returns whether anything changed,
/// so the caller only writes to disk when there was something to migrate.
/// Idempotent either way: skips any target that already has a Mixer node.
fn migrate_mix_sources_to_mixer_nodes(config: &mut AppConfig) -> bool {
    let already_migrated: std::collections::HashSet<String> = config
        .processing_nodes
        .iter()
        .filter(|node| matches!(node.kind, ProcessingNodeSpecKind::Mixer))
        .flat_map(|node| node.output_targets.iter().cloned())
        .collect();

    let mut synthesized = Vec::new();
    let mut migrated_specs = Vec::new();
    for spec in &config.virtual_devices {
        if spec.direction != crate::core::models::DeviceDirection::Input || spec.mix_sources.is_empty() {
            continue;
        }
        let target_system_name = format!("pipe-deck-{}", spec.slug);
        if already_migrated.contains(&target_system_name) {
            continue;
        }
        synthesized.push(ProcessingNodeSpec {
            id: format!("processing-mixer-{}", spec.slug),
            slug: format!("mixer-{}", spec.slug),
            label: format!("{} Mixer", spec.label),
            created_at: spec.created_at.clone(),
            kind: ProcessingNodeSpecKind::Mixer,
            input_sources: spec
                .mix_sources
                .iter()
                .map(|source| ProcessingNodePortSpec {
                    source_system_name: source.system_name.clone(),
                    gain_percent: source.volume_percent,
                    muted: source.muted,
                })
                .collect(),
            output_targets: vec![target_system_name],
            bypassed: false,
        });
        migrated_specs.push(spec.id.clone());
    }

    if synthesized.is_empty() {
        return false;
    }

    for spec in &mut config.virtual_devices {
        if migrated_specs.contains(&spec.id) {
            spec.mix_sources.clear();
        }
    }
    config.processing_nodes.extend(synthesized);
    true
}

/// Serializes any test (in this file or elsewhere in the crate) that mutates
/// the process-wide `PIPE_DECK_CONFIG_DIR` env var via `std::env::set_var`.
/// `cargo test`'s default parallel runner races concurrent `set_var`/
/// `remove_var` calls to the same env var across threads, which manifests as
/// sporadic failures in whichever test happened to read a config dir another
/// thread was mid-swap on. A `static` declared *inside* a `#[test]` fn only
/// guards re-entrant calls to that one function (which can't happen — each
/// test runs once) and provides no cross-test exclusion at all, which is
/// what let this race through despite every affected test already having its
/// own (uselessly scoped) lock. This one is crate-level so every caller
/// shares the same `Mutex`.
#[cfg(test)]
pub(crate) fn lock_config_dir_env() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{EffectChainConfig, VirtualDeviceSpec};
    use std::fs;

    fn with_temp_config<F: FnOnce(&ConfigStore)>(run: F) {
        let _guard = super::lock_config_dir_env();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-config-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &temp_dir);
        let store = ConfigStore::new();
        run(&store);
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
    }

    #[test]
    fn ensure_layout_creates_a_browsable_plugins_directory() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            assert!(store.config_dir().join("plugins").is_dir());
            assert!(store.config_dir().join("profiles").is_dir());
        });
    }

    #[test]
    fn config_without_version_field_defaults_to_one() {
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "preferences:\n  show_system_streams: false\nprofile_index: []\n",
            )
            .unwrap();
            let config = store.load_config().unwrap();
            assert_eq!(config.version, 1);
        });
    }

    #[test]
    fn legacy_config_without_virtual_devices_deserializes() {
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\npreferences:\n  show_system_streams: false\nprofile_index: []\n",
            )
            .unwrap();
            let config = store.load_config().unwrap();
            assert!(config.virtual_devices.is_empty());
            assert!(config.preferences.restore_on_startup);
        });
    }

    #[test]
    fn legacy_config_without_theme_fields_deserializes_to_defaults() {
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\npreferences:\n  show_system_streams: false\nprofile_index: []\n",
            )
            .unwrap();
            let config = store.load_config().unwrap();
            assert_eq!(config.preferences.theme_mode, "dark");
            assert_eq!(config.preferences.dark_scheme, "midnight-deck");
            assert_eq!(config.preferences.light_scheme, "paper-deck");
            assert_eq!(config.preferences.notice_duration_ms, 5000);
        });
    }

    #[test]
    fn theme_preference_setters_round_trip() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            store.set_theme_mode("system").unwrap();
            store.set_dark_scheme("copper-dusk").unwrap();
            store.set_light_scheme("meadow-light").unwrap();
            store.set_notice_duration_ms(8000).unwrap();

            let preferences = store.preferences();
            assert_eq!(preferences.theme_mode, "system");
            assert_eq!(preferences.dark_scheme, "copper-dusk");
            assert_eq!(preferences.light_scheme, "meadow-light");
            assert_eq!(preferences.notice_duration_ms, 8000);
        });
    }

    #[test]
    fn virtual_device_round_trip_persists() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            let spec = VirtualDeviceSpec {
                id: "virtual-test".into(),
                slug: "test".into(),
                label: "Test".into(),
                direction: crate::core::models::DeviceDirection::Output,
                created_at: "2026-07-09T10:00:00Z".into(),
                multi: false,
                mix_sources: Vec::new(),
            };
            store.add_virtual_device(spec.clone()).unwrap();
            let loaded = store.virtual_devices();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0], spec);
            store.remove_virtual_device("virtual-test").unwrap();
            assert!(store.virtual_devices().is_empty());
        });
    }

    #[test]
    fn set_virtual_mic_mix_sources_migrates_into_a_mixer_node_on_next_load() {
        // `set_virtual_mic_mix_sources` itself is now only reachable via
        // `enable_stream_mic_passthrough`'s internal use of the same
        // per-pair-feed-sink mechanism (PD-032 retired the rest of the old
        // mic-mix authoring surface) — its raw write still round-trips, but
        // the *next* load durably migrates it into an equivalent Mixer node,
        // same as any other legacy `mix_sources`.
        use crate::core::models::{MixSourceSpec, ProcessingNodeSpecKind};

        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            let spec = VirtualDeviceSpec {
                id: "virtual-mic".into(),
                slug: "mic".into(),
                label: "Mic".into(),
                direction: crate::core::models::DeviceDirection::Input,
                created_at: "2026-07-09T10:00:00Z".into(),
                multi: false,
                mix_sources: Vec::new(),
            };
            store.add_virtual_device(spec).unwrap();

            let sources = vec![
                MixSourceSpec { system_name: "alsa_input.headset".into(), volume_percent: 60, muted: false },
                MixSourceSpec { system_name: "alsa_input.webcam".into(), volume_percent: 100, muted: true },
            ];
            store
                .set_virtual_mic_mix_sources("pipe-deck-mic", &sources)
                .expect("save mix sources");

            let nodes = store.processing_nodes();
            assert_eq!(nodes.len(), 1);
            assert!(matches!(nodes[0].kind, ProcessingNodeSpecKind::Mixer));
            assert_eq!(nodes[0].input_sources.len(), 2);
            assert!(store.virtual_devices()[0].mix_sources.is_empty());
        });
    }

    #[test]
    fn legacy_mix_sources_migrates_to_an_equivalent_mixer_node_on_load() {
        use crate::core::models::{MixSourceSpec, ProcessingNodeSpecKind};

        with_temp_config(|store| {
            let spec = VirtualDeviceSpec {
                id: "virtual-mic".into(),
                slug: "mic".into(),
                label: "Mic".into(),
                direction: crate::core::models::DeviceDirection::Input,
                created_at: "2026-07-09T10:00:00Z".into(),
                multi: false,
                mix_sources: vec![
                    MixSourceSpec { system_name: "alsa_input.headset".into(), volume_percent: 60, muted: false },
                    MixSourceSpec { system_name: "alsa_input.webcam".into(), volume_percent: 100, muted: true },
                ],
            };
            store.add_virtual_device(spec).unwrap();

            let nodes = store.processing_nodes();
            assert_eq!(nodes.len(), 1);
            assert!(matches!(nodes[0].kind, ProcessingNodeSpecKind::Mixer));
            assert_eq!(nodes[0].output_targets, vec!["pipe-deck-mic".to_string()]);
            assert_eq!(nodes[0].input_sources.len(), 2);
            assert_eq!(nodes[0].input_sources[0].source_system_name, "alsa_input.headset");
            assert_eq!(nodes[0].input_sources[0].gain_percent, 60);
            assert!(!nodes[0].input_sources[0].muted);
            assert_eq!(nodes[0].input_sources[1].source_system_name, "alsa_input.webcam");
            assert!(nodes[0].input_sources[1].muted);

            // Migration is durable — the legacy field is cleared and the
            // clearing is persisted, so re-loading doesn't duplicate the
            // synthesized node or find anything left to migrate again.
            let loaded = store.virtual_devices();
            assert!(loaded[0].mix_sources.is_empty());
            assert_eq!(store.processing_nodes().len(), 1);
        });
    }

    #[test]
    fn legacy_mix_sources_shape_deserializes_at_unity_gain() {
        // Deliberately `direction: output` here, not `input` — an `input`
        // device with non-empty `mix_sources` would immediately migrate into
        // a Mixer node and clear the field (see
        // `legacy_mix_sources_migrates_to_an_equivalent_mixer_node_on_load`),
        // which this test isn't after: it's isolating the legacy
        // bare-`Vec<String>`-to-`MixSourceSpec` deserialization shape itself
        // from that higher-level migration behavior.
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\nprofile_index: []\nvirtual_devices:\n  - id: virtual-mic\n    slug: mic\n    label: Mic\n    direction: output\n    created_at: '2026-07-09T10:00:00Z'\n    mix_sources:\n      - alsa_input.headset\n",
            )
            .unwrap();
            let config = store.load_config().unwrap();
            assert_eq!(
                config.virtual_devices[0].mix_sources,
                vec![crate::core::models::MixSourceSpec::unity("alsa_input.headset")]
            );
        });
    }

    #[test]
    fn legacy_virtual_device_config_with_a_role_key_still_loads() {
        // `virtual_role` (VirtualRole::Bus, #287) was removed outright, not
        // migrated — a plain virtual device can no longer route onward or
        // host effects at all (dedicated processing nodes replace both).
        // Config written before this change may still have the now-unknown
        // `virtual_role: bus` key on disk; it must be silently ignored
        // rather than failing to deserialize.
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\nprofile_index: []\nvirtual_devices:\n  - id: virtual-game-mix\n    slug: game-mix\n    label: Game Mix\n    direction: output\n    created_at: '2026-07-09T10:00:00Z'\n    multi: false\n    virtual_role: bus\n",
            )
            .unwrap();
            let config = store.load_config().unwrap();
            assert_eq!(config.virtual_devices[0].id, "virtual-game-mix");
        });
    }

    #[test]
    fn effect_chain_round_trip_persists() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            let chain = EffectChainConfig {
                stages: vec![crate::core::models::EffectStage::Eq5Band {
                    id: "eq".to_string(),
                    eq_sub: 0,
                    eq_bass: 2,
                    eq_mid: -1,
                    eq_treble: 0,
                    eq_air: 0,
                    output_gain: 0,
                }],
                compressor: crate::core::models::DynamicsStage {
                    enabled: true,
                    threshold_db: -18,
                    ratio_x10: 30,
                    attack_ms: 10,
                    release_ms: 100,
                },
                limiter: crate::core::models::DynamicsStage::default(),
                noise_gate: crate::core::models::DynamicsStage::default(),
                bypassed: false,
                live: true,
            };
            store
                .set_effect_chain("virtual-game", &chain)
                .expect("save chain");
            let loaded = store.effect_chains().expect("load chains");
            assert_eq!(loaded.get("virtual-game"), Some(&chain));
            store
                .remove_effect_chain("virtual-game")
                .expect("remove chain");
            assert!(store.effect_chains().unwrap().is_empty());
        });
    }

    #[test]
    fn legacy_bare_bool_compressor_deserializes_as_enabled_stage() {
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\nprofile_index: []\nplugins:\n  pipe-deck-effects:\n    enabled: true\n    config:\n      chains:\n        virtual-game:\n          compressor: true\n",
            )
            .unwrap();
            let chains = store.effect_chains().expect("load chains");
            let chain = chains.get("virtual-game").expect("chain present");
            assert!(chain.compressor.enabled);
            assert_eq!(chain.compressor.threshold_db, 0);
        });
    }

    #[test]
    fn legacy_flat_eq_fields_migrate_into_a_single_stage() {
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\nprofile_index: []\nplugins:\n  pipe-deck-effects:\n    enabled: true\n    config:\n      chains:\n        virtual-game:\n          eq_bass: 6\n          output_gain: -3\n",
            )
            .unwrap();
            let chains = store.effect_chains().expect("load chains");
            let chain = chains.get("virtual-game").expect("chain present");
            assert_eq!(chain.stages.len(), 1);
            let eq = chain.eq_stage();
            assert_eq!(eq.eq_bass, 6);
            assert_eq!(eq.output_gain, -3);
        });
    }

}
