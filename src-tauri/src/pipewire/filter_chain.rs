use crate::core::models::EffectChainConfig;
use crate::pipewire::adapter::AdapterError;
use std::fs;
use std::path::PathBuf;

pub fn is_pipe_deck_device(system_name: &str) -> bool {
    system_name.starts_with("pipe-deck-")
        && !system_name.starts_with("pipe-deck-feed-")
        && !system_name.starts_with("pipe-deck-split-")
}

/// Persist-only effects hook. Cleans legacy PipeWire drop-ins; does not mutate the live graph.
pub fn sync_all_effects(
    _active: &[(String, EffectChainConfig)],
    _deactivated_system_names: &[String],
) -> Result<(), AdapterError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }

    cleanup_effects_conf_files()
}

/// Remove any Pipe Deck-owned PipeWire drop-ins. Safe to call on startup.
pub fn cleanup_effects_conf_files() -> Result<(), AdapterError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }

    let Some(dir) = effects_conf_dir() else {
        return Ok(());
    };

    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(&dir).map_err(|error| {
        AdapterError::Message(format!("failed to read pipewire config dir: {error}"))
    })? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("99-pipe-deck") {
            let _ = fs::remove_file(entry.path());
        }
    }

    Ok(())
}

pub fn apply_effect_chain(
    device_system_name: &str,
    config: &EffectChainConfig,
) -> Result<Option<String>, AdapterError> {
    let active = if config.is_active() {
        vec![(device_system_name.to_string(), config.clone())]
    } else {
        Vec::new()
    };
    let deactivated = if config.is_active() {
        Vec::new()
    } else {
        vec![device_system_name.to_string()]
    };

    sync_all_effects(&active, &deactivated)?;
    Ok(None)
}

fn effects_conf_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/pipewire/pipewire.conf.d"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_pipe_deck_virtual_devices() {
        assert!(is_pipe_deck_device("pipe-deck-game-mix"));
        assert!(!is_pipe_deck_device("pipe-deck-split-firefox"));
        assert!(!is_pipe_deck_device("alsa_output.pci"));
    }
}
