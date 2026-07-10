use crate::config::ConfigStore;
use crate::core::models::{DeviceDirection, Profile, RestoreResult, RuntimeGraph, VirtualDeviceSpec};
use crate::pipewire::adapter::PipeWireAdapter;
use crate::pipewire::pactl::{self, PactlVirtualModule};
use crate::pipewire::virtual_devices::{slugify, VirtualDeviceRegistry};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RestoreError {
    #[error("config error: {0}")]
    Config(String),
    #[error("adapter error: {0}")]
    Adapter(String),
}

pub fn restore_session(registry: &Arc<VirtualDeviceRegistry>) -> Result<RestoreResult, RestoreError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(RestoreResult {
            created: Vec::new(),
            adopted: Vec::new(),
            removed_orphans: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        });
    }

    let store = ConfigStore::new();
    let mut config = store
        .load_config()
        .map_err(|error| RestoreError::Config(error.to_string()))?;

    if !config.preferences.restore_on_startup {
        registry
            .discover_from_pactl()
            .map_err(|error| RestoreError::Adapter(error.to_string()))?;
        return Ok(RestoreResult {
            created: Vec::new(),
            adopted: Vec::new(),
            removed_orphans: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        });
    }

    let mut result = RestoreResult {
        created: Vec::new(),
        adopted: Vec::new(),
        removed_orphans: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    let modules = pactl::list_pipe_deck_modules()
        .map_err(|error| RestoreError::Adapter(error.to_string()))?;
    let module_by_name: HashMap<String, PactlVirtualModule> = modules
        .into_iter()
        .map(|module| (module.system_name.clone(), module))
        .collect();

    if config.virtual_devices.is_empty() && !module_by_name.is_empty() {
        let now = Utc::now().to_rfc3339();
        config.virtual_devices = module_by_name
            .values()
            .map(|module| VirtualDeviceSpec {
                id: module.device_id.clone(),
                slug: module
                    .system_name
                    .strip_prefix("pipe-deck-")
                    .unwrap_or(&module.system_name)
                    .to_string(),
                label: module.label.clone(),
                direction: module.direction.clone(),
                created_at: now.clone(),
                multi: false,
            })
            .collect();
        result.warnings.push(
            "Migrated existing Pipe Deck virtual devices into config.yaml".into(),
        );
    }

    let configured_names: HashSet<String> = config
        .virtual_devices
        .iter()
        .map(|spec| format!("pipe-deck-{}", spec.slug))
        .collect();

    for spec in &config.virtual_devices {
        let system_name = format!("pipe-deck-{}", spec.slug);
        if module_by_name.contains_key(&system_name) {
            result.adopted.push(system_name);
            continue;
        }

        match create_virtual_from_spec(spec) {
            Ok(()) => result.created.push(system_name),
            Err(error) => result.errors.push(format!("{system_name}: {error}")),
        }
    }

    for (system_name, module) in &module_by_name {
        if configured_names.contains(system_name) {
            continue;
        }
        result.warnings.push(format!(
            "Removing orphaned Pipe Deck module not listed in config: {system_name}"
        ));
        if let Err(error) = pactl::unload_module(&module.module_id) {
            result
                .errors
                .push(format!("failed to unload orphan {system_name}: {error}"));
        } else {
            result.removed_orphans.push(system_name.clone());
        }
    }

    if result.warnings.iter().any(|warning| warning.contains("Migrated")) {
        store
            .save_virtual_devices(&config.virtual_devices)
            .map_err(|error| RestoreError::Config(error.to_string()))?;
    }

    registry
        .discover_from_pactl()
        .map_err(|error| RestoreError::Adapter(error.to_string()))?;

    Ok(result)
}

pub fn restore_profile_virtual_devices(
    registry: &Arc<VirtualDeviceRegistry>,
    profile: &Profile,
) -> Result<RestoreResult, RestoreError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(RestoreResult {
            created: Vec::new(),
            adopted: Vec::new(),
            removed_orphans: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        });
    }

    if profile.device_assumptions.is_empty() {
        return Ok(RestoreResult {
            created: Vec::new(),
            adopted: Vec::new(),
            removed_orphans: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        });
    }

    let store = ConfigStore::new();
    let config = store
        .load_config()
        .map_err(|error| RestoreError::Config(error.to_string()))?;
    let spec_by_id: HashMap<String, VirtualDeviceSpec> = config
        .virtual_devices
        .iter()
        .map(|spec| (spec.id.clone(), spec.clone()))
        .collect();

    let modules = pactl::list_pipe_deck_modules()
        .map_err(|error| RestoreError::Adapter(error.to_string()))?;
    let present: HashSet<String> = modules.into_iter().map(|module| module.system_name).collect();

    let mut result = RestoreResult {
        created: Vec::new(),
        adopted: Vec::new(),
        removed_orphans: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    for device_id in profile.device_assumptions.keys() {
        let Some(spec) = spec_by_id.get(device_id) else {
            result.warnings.push(format!(
                "Profile references virtual device {device_id} that is not in config"
            ));
            continue;
        };
        let system_name = format!("pipe-deck-{}", spec.slug);
        if present.contains(&system_name) {
            result.adopted.push(system_name);
            continue;
        }
        match create_virtual_from_spec(spec) {
            Ok(()) => result.created.push(system_name),
            Err(error) => result.errors.push(format!("{system_name}: {error}")),
        }
    }

    registry
        .discover_from_pactl()
        .map_err(|error| RestoreError::Adapter(error.to_string()))?;

    Ok(result)
}

pub fn apply_persisted_routes(registry: &Arc<VirtualDeviceRegistry>) -> Result<(), RestoreError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }

    let adapter = crate::pipewire::live::LivePipeWireAdapter::new()
        .map_err(|error| RestoreError::Adapter(error.to_string()))?;
    let mut graph = adapter
        .fetch_graph()
        .map_err(|error| RestoreError::Adapter(error.to_string()))?;
    merge_registry_into_graph(&mut graph, registry);

    let overrides = HashSet::new();
    let device_overrides = HashSet::new();
    let ctx = crate::core::rule_engine::ApplyRulesContext {
        manual_overrides: &overrides,
        device_manual_overrides: &device_overrides,
        dry_run: false,
        mock_graph_only: false,
        limit_to_identities: None,
    };
    crate::pipewire::live::apply_graph_routing(&mut graph, &ctx);
    Ok(())
}

pub fn merge_registry_into_graph(graph: &mut RuntimeGraph, registry: &VirtualDeviceRegistry) {
    let multi_by_name: std::collections::HashMap<String, bool> = ConfigStore::new()
        .virtual_devices()
        .into_iter()
        .map(|spec| (format!("pipe-deck-{}", spec.slug), spec.multi))
        .collect();

    for entry in registry.list_devices() {
        let sink_mode = if entry.direction == DeviceDirection::Output {
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
            device.id = entry.device_id.clone();
            device.label = entry.label.clone();
            device.kind = crate::core::models::DeviceKind::Virtual;
            device.direction = entry.direction.clone();
            device.sink_mode = sink_mode;
        } else {
            let mut device = entry.to_device();
            device.sink_mode = sink_mode;
            graph.devices.push(device);
        }
    }

    crate::pipewire::live::apply_device_aliases(&mut graph.devices);
}

pub fn spec_from_create_result(
    device_id: &str,
    system_name: &str,
    label: &str,
    direction: DeviceDirection,
    multi: bool,
) -> VirtualDeviceSpec {
    let slug = system_name
        .strip_prefix("pipe-deck-")
        .map(|value| value.to_string())
        .unwrap_or_else(|| slugify(label));
    VirtualDeviceSpec {
        id: device_id.to_string(),
        slug,
        label: label.to_string(),
        direction,
        created_at: Utc::now().to_rfc3339(),
        multi,
    }
}

fn create_virtual_from_spec(spec: &VirtualDeviceSpec) -> Result<(), String> {
    let system_name = format!("pipe-deck-{}", spec.slug);
    match spec.direction {
        DeviceDirection::Input => pactl::create_virtual_source(&system_name, &spec.label)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        DeviceDirection::Output | DeviceDirection::Duplex => {
            pactl::create_null_sink(&system_name, &spec.label)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_from_create_result_uses_slug_from_system_name() {
        let spec = spec_from_create_result(
            "virtual-game-mix",
            "pipe-deck-game-mix",
            "Game Mix",
            DeviceDirection::Output,
            true,
        );
        assert_eq!(spec.id, "virtual-game-mix");
        assert_eq!(spec.slug, "game-mix");
        assert_eq!(spec.label, "Game Mix");
        assert!(spec.multi);
    }

    #[test]
    fn slugify_matches_virtual_device_naming() {
        assert_eq!(slugify("Game Mix"), "game-mix");
        assert_eq!(slugify("My Mic!!!"), "my-mic");
    }
}
