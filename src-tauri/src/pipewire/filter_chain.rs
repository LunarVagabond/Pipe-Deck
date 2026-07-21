use crate::backend::BackendError;
use crate::backend::linux::pactl;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn is_pipe_deck_device(system_name: &str) -> bool {
    system_name.starts_with("pipe-deck-")
        && !system_name.starts_with("pipe-deck-feed-")
        && !system_name.starts_with("pipe-deck-split-")
}

/// One-time migration cleanup: removes any Pipe Deck-owned PipeWire drop-ins
/// left over from before #149's cutover to native effects transport (both
/// the pre-issue-#64 `pipewire.conf.d` location and the later
/// `filter-chain.conf.d` one) — nothing writes either anymore, but a
/// pre-existing file would otherwise sit there indefinitely, or (for the
/// `filter-chain.conf.d` one, if the user's distro still runs
/// `filter-chain.service`) keep recreating a ghost sink on every restart of
/// that unrelated service. Safe to call on every startup.
pub fn cleanup_effects_conf_files() -> Result<(), BackendError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }

    for dir in [effects_conf_dir(), filter_chain_conf_dir()].into_iter().flatten() {
        if !dir.is_dir() {
            continue;
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
    }

    Ok(())
}

fn effects_conf_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/pipewire/pipewire.conf.d"))
}

/// Where the now-retired restart-based mechanism used to write its
/// filter-chain.service conf.d drop-ins — kept only so
/// `cleanup_effects_conf_files` can purge any left over from before #149.
fn filter_chain_conf_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PIPE_DECK_FILTER_CHAIN_CONF_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/pipewire/filter-chain.conf.d"))
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

/// Polls for a sink named `system_name` to (re)appear after
/// `revert_to_plain_device` recreates it, so the caller can confirm the
/// plain device is actually back before re-linking anything to it.
pub fn wait_for_sink(system_name: &str, timeout: Duration) -> Result<(), BackendError> {
    let start = Instant::now();
    loop {
        if pactl::sink_exists(system_name).unwrap_or(false) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{system_name} did not reappear within {timeout:?}"
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Polls for a source named `system_name` to (re)appear after
/// `revert_to_plain_device` recreates it — the capture-direction (virtual
/// input) counterpart to `wait_for_sink`.
pub fn wait_for_source(system_name: &str, timeout: Duration) -> Result<(), BackendError> {
    let start = Instant::now();
    loop {
        if pactl::source_exists(system_name).unwrap_or(false) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{system_name} did not reappear as a source within {timeout:?}"
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
