use crate::core::models::{AppConfig, DeviceAliasEntry, Preferences, ProfileIndexEntry};
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

    fn default_config() -> AppConfig {
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
        }
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
        self.save_config(&config)
    }

    pub fn set_show_system_streams(&self, show: bool) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.preferences.show_system_streams = show;
        self.save_config(&config)
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileIndexEntry>, ConfigError> {
        Ok(self.load_config()?.profile_index)
    }
}
