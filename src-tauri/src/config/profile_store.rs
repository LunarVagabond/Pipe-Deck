use crate::core::models::{Profile, ProfileExportManifest, ProfileIndexEntry};
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::{Builder, Header};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("failed to read profile: {0}")]
    Read(String),
    #[error("failed to write profile: {0}")]
    Write(String),
    #[error("profile validation failed: {0}")]
    Validation(String),
    #[error("profile not found: {0}")]
    NotFound(String),
}

pub struct ProfileStore {
    config_dir: PathBuf,
}

impl ProfileStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.config_dir.join("profiles")
    }

    pub fn profile_path(&self, file: &str) -> PathBuf {
        self.config_dir.join(file)
    }

    pub fn ensure_default_profile(&self) -> Result<(), ProfileError> {
        let profiles_dir = self.profiles_dir();
        fs::create_dir_all(&profiles_dir)
            .map_err(|error| ProfileError::Write(format!("{profiles_dir:?}: {error}")))?;

        let default_path = profiles_dir.join("default.yaml");
        if default_path.exists() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let profile = Profile {
            version: 1,
            id: "default".into(),
            name: "Default".into(),
            created: now.clone(),
            updated: now,
            routing_intents: vec![],
            volume_state: Default::default(),
            device_assumptions: Default::default(),
        };
        self.save_profile_at(&default_path, &profile)
    }

    pub fn load_profile(&self, entry: &ProfileIndexEntry) -> Result<Profile, ProfileError> {
        let path = self.profile_path(&entry.file);
        if !path.exists() {
            return Err(ProfileError::NotFound(format!("{path:?}")));
        }

        let contents = fs::read_to_string(&path)
            .map_err(|error| ProfileError::Read(format!("{path:?}: {error}")))?;
        let profile: Profile = serde_yaml::from_str(&contents)
            .map_err(|error| ProfileError::Read(format!("{path:?}: {error}")))?;
        validate_profile(&profile)?;
        Ok(profile)
    }

    pub fn load_profile_by_id(
        &self,
        id: &str,
        index: &[ProfileIndexEntry],
    ) -> Result<Profile, ProfileError> {
        let entry = index
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| ProfileError::NotFound(id.to_string()))?;
        self.load_profile(entry)
    }

    pub fn save_profile(&self, entry: &ProfileIndexEntry, profile: &Profile) -> Result<(), ProfileError> {
        validate_profile(profile)?;
        let path = self.profile_path(&entry.file);
        self.save_profile_at(&path, profile)
    }

    pub fn save_profile_as(
        &self,
        id: &str,
        name: &str,
        profile: &Profile,
    ) -> Result<ProfileIndexEntry, ProfileError> {
        validate_profile(profile)?;
        fs::create_dir_all(self.profiles_dir())
            .map_err(|error| ProfileError::Write(error.to_string()))?;

        let file = format!("profiles/{id}.yaml");
        let path = self.profile_path(&file);
        self.save_profile_at(&path, profile)?;

        Ok(ProfileIndexEntry {
            id: id.to_string(),
            name: name.to_string(),
            file,
        })
    }

    pub fn import_profile_file(&self, source: &Path) -> Result<ProfileIndexEntry, ProfileError> {
        let contents = fs::read_to_string(source)
            .map_err(|error| ProfileError::Read(format!("{source:?}: {error}")))?;
        let profile: Profile = serde_yaml::from_str(&contents)
            .map_err(|error| ProfileError::Read(format!("{source:?}: {error}")))?;
        validate_profile(&profile)?;

        fs::create_dir_all(self.profiles_dir())
            .map_err(|error| ProfileError::Write(error.to_string()))?;

        let file = format!("profiles/{}.yaml", profile.id);
        let dest = self.profile_path(&file);
        self.save_profile_at(&dest, &profile)?;

        Ok(ProfileIndexEntry {
            id: profile.id.clone(),
            name: profile.name.clone(),
            file,
        })
    }

    pub fn export_profile_archive(
        &self,
        entry: &ProfileIndexEntry,
        destination: &Path,
    ) -> Result<(), ProfileError> {
        let profile = self.load_profile(entry)?;
        let manifest = ProfileExportManifest {
            version: 1,
            exported_at: Utc::now().to_rfc3339(),
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
        };

        let file = File::create(destination)
            .map_err(|error| ProfileError::Write(format!("{destination:?}: {error}")))?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut archive = Builder::new(encoder);

        let profile_yaml = serde_yaml::to_string(&profile)
            .map_err(|error| ProfileError::Write(error.to_string()))?;
        append_tar_entry(&mut archive, "profile.yaml", profile_yaml.as_bytes())?;

        let manifest_yaml = serde_yaml::to_string(&manifest)
            .map_err(|error| ProfileError::Write(error.to_string()))?;
        append_tar_entry(&mut archive, "manifest.yaml", manifest_yaml.as_bytes())?;

        archive
            .into_inner()
            .map_err(|error| ProfileError::Write(error.to_string()))?
            .finish()
            .map_err(|error| ProfileError::Write(error.to_string()))?;

        Ok(())
    }

    fn save_profile_at(&self, path: &Path, profile: &Profile) -> Result<(), ProfileError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ProfileError::Write(format!("{parent:?}: {error}")))?;
        }

        let contents = serde_yaml::to_string(profile)
            .map_err(|error| ProfileError::Write(error.to_string()))?;
        fs::write(path, contents)
            .map_err(|error| ProfileError::Write(format!("{path:?}: {error}")))
    }
}

pub fn validate_profile(profile: &Profile) -> Result<(), ProfileError> {
    if profile.version == 0 {
        return Err(ProfileError::Validation("version must be >= 1".into()));
    }
    if profile.id.trim().is_empty() {
        return Err(ProfileError::Validation("profile id is required".into()));
    }
    if profile.name.trim().is_empty() {
        return Err(ProfileError::Validation("profile name is required".into()));
    }
    for intent in &profile.routing_intents {
        if intent.stream_id.trim().is_empty() || intent.target_device_id.trim().is_empty() {
            return Err(ProfileError::Validation(
                "routing intents require stream_id and target_device_id".into(),
            ));
        }
    }
    for (device_id, state) in &profile.volume_state {
        if device_id.trim().is_empty() {
            return Err(ProfileError::Validation(
                "volume_state keys must be non-empty device ids".into(),
            ));
        }
        if state.volume_percent > 100 {
            return Err(ProfileError::Validation(
                "volume_percent must be between 0 and 100".into(),
            ));
        }
    }
    Ok(())
}

fn append_tar_entry<W: Write>(
    archive: &mut Builder<W>,
    name: &str,
    data: &[u8],
) -> Result<(), ProfileError> {
    let mut header = Header::new_gnu();
    header.set_path(name)
        .map_err(|error| ProfileError::Write(error.to_string()))?;
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append(&header, data)
        .map_err(|error| ProfileError::Write(error.to_string()))
}

pub fn import_profile_archive(source: &Path, profiles_dir: &Path) -> Result<ProfileIndexEntry, ProfileError> {
    let file = File::open(source)
        .map_err(|error| ProfileError::Read(format!("{source:?}: {error}")))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    let mut profile_yaml = String::new();
    for entry in archive
        .entries()
        .map_err(|error| ProfileError::Read(error.to_string()))?
    {
        let mut entry = entry.map_err(|error| ProfileError::Read(error.to_string()))?;
        let path = entry
            .path()
            .map_err(|error| ProfileError::Read(error.to_string()))?;
        if path.file_name().and_then(|name| name.to_str()) == Some("profile.yaml") {
            entry
                .read_to_string(&mut profile_yaml)
                .map_err(|error| ProfileError::Read(error.to_string()))?;
            break;
        }
    }

    if profile_yaml.is_empty() {
        return Err(ProfileError::Read("archive missing profile.yaml".into()));
    }

    let profile: Profile = serde_yaml::from_str(&profile_yaml)
        .map_err(|error| ProfileError::Read(error.to_string()))?;
    validate_profile(&profile)?;

    fs::create_dir_all(profiles_dir)
        .map_err(|error| ProfileError::Write(error.to_string()))?;

    let file = format!("profiles/{}.yaml", profile.id);
    let dest = profiles_dir.join(format!("{}.yaml", profile.id));
    fs::write(&dest, profile_yaml)
        .map_err(|error| ProfileError::Write(format!("{dest:?}: {error}")))?;

    Ok(ProfileIndexEntry {
        id: profile.id,
        name: profile.name,
        file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{RoutingIntent, VolumeStateEntry};

    #[test]
    fn validates_profile_shape() {
        let profile = Profile {
            version: 1,
            id: "gaming".into(),
            name: "Gaming".into(),
            created: "2026-07-09T10:00:00Z".into(),
            updated: "2026-07-09T10:00:00Z".into(),
            routing_intents: vec![RoutingIntent {
                stream_id: "node-1".into(),
                target_device_id: "node-2".into(),
            }],
            volume_state: [(
                "node-2".into(),
                VolumeStateEntry {
                    volume_percent: 80,
                    muted: false,
                },
            )]
            .into_iter()
            .collect(),
            device_assumptions: Default::default(),
        };

        validate_profile(&profile).expect("valid profile");
    }

    #[test]
    fn rejects_invalid_volume_percent() {
        let profile = Profile {
            version: 1,
            id: "bad".into(),
            name: "Bad".into(),
            created: "2026-07-09T10:00:00Z".into(),
            updated: "2026-07-09T10:00:00Z".into(),
            routing_intents: vec![],
            volume_state: [(
                "node-1".into(),
                VolumeStateEntry {
                    volume_percent: 150,
                    muted: false,
                },
            )]
            .into_iter()
            .collect(),
            device_assumptions: Default::default(),
        };

        assert!(validate_profile(&profile).is_err());
    }
}
