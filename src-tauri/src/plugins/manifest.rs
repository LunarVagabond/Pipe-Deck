use crate::core::models::PluginManifest;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const SUPPORTED_API_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to read manifest: {0}")]
    Read(String),
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub manifest: PluginManifest,
    pub root: PathBuf,
    pub entry_path: PathBuf,
}

pub fn load_manifest(path: &Path) -> Result<PluginManifest, ManifestError> {
    let contents = fs::read_to_string(path)
        .map_err(|error| ManifestError::Read(format!("{path:?}: {error}")))?;
    let manifest: PluginManifest = serde_yaml::from_str(&contents)
        .map_err(|error| ManifestError::Invalid(format!("{path:?}: {error}")))?;
    validate_manifest(&manifest, path.parent())?;
    Ok(manifest)
}

pub fn validate_manifest(
    manifest: &PluginManifest,
    root: Option<&Path>,
) -> Result<(), ManifestError> {
    if manifest.id.trim().is_empty() {
        return Err(ManifestError::Invalid("id is required".into()));
    }
    if manifest.name.trim().is_empty() {
        return Err(ManifestError::Invalid("name is required".into()));
    }
    if manifest.entry.trim().is_empty() {
        return Err(ManifestError::Invalid("entry is required".into()));
    }
    if manifest.api_version != SUPPORTED_API_VERSION {
        return Err(ManifestError::Invalid(format!(
            "unsupported api_version {} (host supports {SUPPORTED_API_VERSION})",
            manifest.api_version
        )));
    }
    for capability in &manifest.capabilities {
        if !crate::plugins::capabilities::is_known(capability) {
            return Err(ManifestError::Invalid(format!(
                "unknown capability: {capability}"
            )));
        }
    }
    if let Some(root) = root {
        let entry = root.join(&manifest.entry);
        if !entry.exists() {
            return Err(ManifestError::Invalid(format!(
                "entry binary not found: {}",
                entry.display()
            )));
        }
    }
    Ok(())
}

pub fn discover_in_dir(dir: &Path) -> Vec<DiscoveredPlugin> {
    let mut plugins = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return plugins,
    };

    for entry in entries.flatten() {
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let manifest_path = root.join("plugin.yaml");
        if !manifest_path.exists() {
            continue;
        }
        let Ok(manifest) = load_manifest(&manifest_path) else {
            continue;
        };
        let entry_path = root.join(&manifest.entry);
        plugins.push(DiscoveredPlugin {
            manifest,
            root,
            entry_path,
        });
    }

    plugins.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    plugins
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::PluginManifest;

    #[test]
    fn rejects_unsupported_api_version() {
        let manifest = PluginManifest {
            id: "test".into(),
            name: "Test".into(),
            version: "0.1.0".into(),
            api_version: 99,
            entry: "bin/test".into(),
            capabilities: vec!["graph.read".into()],
            description: None,
            bundled: false,
        };
        let error = validate_manifest(&manifest, None).unwrap_err();
        assert!(error.to_string().contains("unsupported api_version"));
    }

    #[test]
    fn loads_manifest_from_yaml_file() {
        let dir = std::env::temp_dir().join(format!("pipe-deck-manifest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin/echo"), b"#!/bin/sh\n").unwrap();
        let yaml = r#"id: echo
name: Echo
version: 0.1.0
api_version: 1
entry: bin/echo
capabilities:
  - graph.read
"#;
        fs::write(dir.join("plugin.yaml"), yaml).unwrap();
        let manifest = load_manifest(&dir.join("plugin.yaml")).unwrap();
        assert_eq!(manifest.id, "echo");
        let _ = fs::remove_dir_all(&dir);
    }
}
