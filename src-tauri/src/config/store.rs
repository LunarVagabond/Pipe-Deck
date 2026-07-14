use crate::core::models::{
    connection_effect_key, AppConfig, ConnectionEffectKind, DeviceAliasEntry, EffectChainConfig,
    PluginEntry, Preferences, ProfileIndexEntry, Rule, RoutingRulesConfig, VirtualDeviceSpec,
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
            plugins: HashMap::new(),
            connection_effects: HashMap::new(),
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
        serde_yaml::from_str(&contents)
            .map_err(|error| ConfigError::Read(format!("{path:?}: {error}")))
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

    /// Reads the persisted effects for one connection (issue #105), if any.
    pub fn connection_effects(
        &self,
        source_system_name: &str,
        target_system_name: &str,
    ) -> Result<Vec<ConnectionEffectKind>, ConfigError> {
        let config = self.load_config()?;
        let key = connection_effect_key(source_system_name, target_system_name);
        Ok(config.connection_effects.get(&key).cloned().unwrap_or_default())
    }

    /// Every persisted connection's effects, keyed the same way as
    /// `AppConfig::connection_effects` — used to recreate backing feed sinks
    /// on startup/profile-swap (see `core::restore`).
    pub fn all_connection_effects(
        &self,
    ) -> Result<std::collections::HashMap<String, Vec<ConnectionEffectKind>>, ConfigError> {
        Ok(self.load_config()?.connection_effects)
    }

    /// Adds or replaces a connection's effect list (issue #105). An empty
    /// `effects` list removes the entry entirely rather than persisting a
    /// bare empty vec.
    pub fn set_connection_effects(
        &self,
        source_system_name: &str,
        target_system_name: &str,
        effects: Vec<ConnectionEffectKind>,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let key = connection_effect_key(source_system_name, target_system_name);
        if effects.is_empty() {
            config.connection_effects.remove(&key);
        } else {
            config.connection_effects.insert(key, effects);
        }
        self.save_config(&config)
    }

    /// Updates the persisted volume for a connection's `Volume` effect
    /// without touching any other effect kind attached to the same
    /// connection. No-ops if the connection has no `Volume` effect yet —
    /// callers add the effect first (see issue #108's `add_connection_effect`).
    pub fn update_connection_volume(
        &self,
        source_system_name: &str,
        target_system_name: &str,
        percent: u8,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let key = connection_effect_key(source_system_name, target_system_name);
        if let Some(effects) = config.connection_effects.get_mut(&key) {
            for effect in effects.iter_mut() {
                let ConnectionEffectKind::Volume { volume_percent, .. } = effect;
                *volume_percent = percent;
            }
        }
        self.save_config(&config)
    }

    /// Updates the persisted mute state for a connection's `Volume` effect.
    /// See `update_connection_volume` for the no-op-if-absent behavior.
    pub fn update_connection_mute(
        &self,
        source_system_name: &str,
        target_system_name: &str,
        muted_value: bool,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let key = connection_effect_key(source_system_name, target_system_name);
        if let Some(effects) = config.connection_effects.get_mut(&key) {
            for effect in effects.iter_mut() {
                let ConnectionEffectKind::Volume { muted, .. } = effect;
                *muted = muted_value;
            }
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
            .or_insert_with(PluginEntry::default);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{EffectChainConfig, VirtualDeviceSpec};
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    fn with_temp_config<F: FnOnce(&ConfigStore)>(run: F) {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
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
        });
    }

    #[test]
    fn theme_preference_setters_round_trip() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            store.set_theme_mode("system").unwrap();
            store.set_dark_scheme("copper-dusk").unwrap();
            store.set_light_scheme("meadow-light").unwrap();

            let preferences = store.preferences();
            assert_eq!(preferences.theme_mode, "system");
            assert_eq!(preferences.dark_scheme, "copper-dusk");
            assert_eq!(preferences.light_scheme, "meadow-light");
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
    fn mix_source_volume_round_trip_persists() {
        use crate::core::models::MixSourceSpec;

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

            let loaded = store.virtual_devices();
            assert_eq!(loaded[0].mix_sources, sources);
        });
    }

    #[test]
    fn legacy_mix_sources_shape_deserializes_at_unity_gain() {
        with_temp_config(|store| {
            fs::create_dir_all(store.config_dir()).unwrap();
            fs::write(
                store.config_dir().join("config.yaml"),
                "version: 1\nprofile_index: []\nvirtual_devices:\n  - id: virtual-mic\n    slug: mic\n    label: Mic\n    direction: input\n    created_at: '2026-07-09T10:00:00Z'\n    mix_sources:\n      - alsa_input.headset\n",
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
    fn effect_chain_round_trip_persists() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            let chain = EffectChainConfig {
                eq_sub: 0,
                eq_bass: 2,
                eq_mid: -1,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
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
    fn connection_effects_round_trip_persists() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();

            store
                .set_connection_effects(
                    "stream-spotify",
                    "alsa_output.speakers",
                    vec![ConnectionEffectKind::Volume {
                        volume_percent: 80,
                        muted: false,
                    }],
                )
                .expect("save connection effects");

            let effects = store
                .connection_effects("stream-spotify", "alsa_output.speakers")
                .expect("load connection effects");
            assert_eq!(
                effects,
                vec![ConnectionEffectKind::Volume {
                    volume_percent: 80,
                    muted: false
                }]
            );

            // A different pair with no effects yet reads back empty, not an error.
            assert!(store
                .connection_effects("stream-other", "alsa_output.speakers")
                .unwrap()
                .is_empty());
        });
    }

    #[test]
    fn connection_volume_and_mute_updates_do_not_touch_other_connections() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            store
                .set_connection_effects(
                    "mic",
                    "pipe-deck-mic",
                    vec![ConnectionEffectKind::Volume {
                        volume_percent: 100,
                        muted: false,
                    }],
                )
                .unwrap();
            store
                .set_connection_effects(
                    "stream-spotify",
                    "pipe-deck-mic",
                    vec![ConnectionEffectKind::Volume {
                        volume_percent: 100,
                        muted: false,
                    }],
                )
                .unwrap();

            store.update_connection_volume("mic", "pipe-deck-mic", 40).unwrap();
            store.update_connection_mute("mic", "pipe-deck-mic", true).unwrap();

            let mic_effects = store.connection_effects("mic", "pipe-deck-mic").unwrap();
            assert_eq!(
                mic_effects,
                vec![ConnectionEffectKind::Volume {
                    volume_percent: 40,
                    muted: true
                }]
            );

            // The other connection's effect is untouched.
            let spotify_effects = store
                .connection_effects("stream-spotify", "pipe-deck-mic")
                .unwrap();
            assert_eq!(
                spotify_effects,
                vec![ConnectionEffectKind::Volume {
                    volume_percent: 100,
                    muted: false
                }]
            );
        });
    }

    #[test]
    fn setting_empty_effects_removes_the_entry_rather_than_persisting_a_bare_vec() {
        with_temp_config(|store| {
            store.ensure_layout().unwrap();
            store
                .set_connection_effects(
                    "mic",
                    "pipe-deck-mic",
                    vec![ConnectionEffectKind::Volume {
                        volume_percent: 100,
                        muted: false,
                    }],
                )
                .unwrap();

            store.set_connection_effects("mic", "pipe-deck-mic", Vec::new()).unwrap();

            assert!(store.all_connection_effects().unwrap().is_empty());
        });
    }
}
