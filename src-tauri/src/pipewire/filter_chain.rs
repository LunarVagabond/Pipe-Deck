use crate::backend::linux::pactl;
use crate::backend::BackendError;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

    for dir in [effects_conf_dir(), filter_chain_conf_dir()]
        .into_iter()
        .flatten()
    {
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
