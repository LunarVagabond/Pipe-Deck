use crate::core::models::EffectChainConfig;
use crate::backend::BackendError;
use crate::backend::linux::{pactl, pw_link};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn is_pipe_deck_device(system_name: &str) -> bool {
    system_name.starts_with("pipe-deck-")
        && !system_name.starts_with("pipe-deck-feed-")
        && !system_name.starts_with("pipe-deck-split-")
}

/// Persist-only effects hook. Cleans legacy PipeWire drop-ins; does not mutate the live graph.
pub fn sync_all_effects(
    _active: &[(String, EffectChainConfig)],
    _deactivated_system_names: &[String],
) -> Result<(), BackendError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }

    cleanup_effects_conf_files()
}

/// Remove any Pipe Deck-owned PipeWire drop-ins. Safe to call on startup.
pub fn cleanup_effects_conf_files() -> Result<(), BackendError> {
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
        BackendError::Message(format!("failed to read pipewire config dir: {error}"))
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
) -> Result<Option<String>, BackendError> {
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

/// Where live effects conf.d drop-ins actually live — the dedicated
/// `filter-chain.service` daemon's config directory (per its own base conf's
/// documented convention), *not* the main `pipewire.conf.d` used by
/// `effects_conf_dir()` above (that one is legacy-cleanup-only; nothing
/// current ever writes there).
pub fn filter_chain_conf_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/pipewire/filter-chain.conf.d"))
}

pub fn conf_path_for_device(device_system_name: &str) -> Option<PathBuf> {
    let dir = filter_chain_conf_dir()?;
    let slug = device_system_name.strip_prefix("pipe-deck-").unwrap_or(device_system_name);
    Some(dir.join(format!("99-pipe-deck-effects-{slug}.conf")))
}

pub fn effect_output_name_for_device(device_system_name: &str) -> String {
    format!("effect_output.{device_system_name}")
}

/// The raw-audio inlet name for a capture-direction (virtual input/mic)
/// effect chain (PD-024) — the counterpart to `effect_output_name_for_device`
/// for the reversed capture template, see `fx_validate::render_conf_capture`.
pub fn effect_input_name_for_device(device_system_name: &str) -> String {
    format!("effect_input.{device_system_name}")
}

/// Polls for a sink named `system_name` to (re)appear after a filter-chain
/// restart, so Structural Apply can confirm the swap actually took before
/// re-linking anything downstream.
pub fn wait_for_sink(system_name: &str, timeout: Duration) -> Result<(), BackendError> {
    let start = Instant::now();
    loop {
        if pactl::sink_exists(system_name).unwrap_or(false) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{system_name} did not reappear within {timeout:?} after the effects restart"
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Polls for the filter-chain's *playback* side (`effect_output.{system_name}`)
/// to actually register its output ports after a restart. `wait_for_sink` only
/// confirms the capture sink exists; the paired playback node backing
/// `effect_output.*` can take a beat longer to show up, and relinking
/// downstream targets against it before then fails (and triggers an
/// avoidable — if correct — Structural Apply rollback).
pub fn wait_for_effect_output_ports(system_name: &str, timeout: Duration) -> Result<(), BackendError> {
    let effect_output_name = effect_output_name_for_device(system_name);
    let start = Instant::now();
    loop {
        if pw_link::has_output_ports(&effect_output_name) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{effect_output_name} did not register output ports within {timeout:?} after the effects restart"
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Polls for a source named `system_name` to (re)appear after a filter-chain
/// restart — the capture-direction (virtual input) counterpart to
/// `wait_for_sink`.
pub fn wait_for_source(system_name: &str, timeout: Duration) -> Result<(), BackendError> {
    let start = Instant::now();
    loop {
        if pactl::source_exists(system_name).unwrap_or(false) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{system_name} did not reappear as a source within {timeout:?} after the effects restart"
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Polls for the filter-chain's raw-audio inlet (`effect_input.{system_name}`)
/// to register its input ports after a restart — the capture-direction
/// counterpart to `wait_for_effect_output_ports`, confirming the inlet is
/// ready to accept the mic-mix feed relink before it's attempted.
pub fn wait_for_effect_input_ports(system_name: &str, timeout: Duration) -> Result<(), BackendError> {
    let effect_input_name = effect_input_name_for_device(system_name);
    let start = Instant::now();
    loop {
        if pw_link::has_input_ports(&effect_input_name) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{effect_input_name} did not register input ports within {timeout:?} after the effects restart"
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
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
