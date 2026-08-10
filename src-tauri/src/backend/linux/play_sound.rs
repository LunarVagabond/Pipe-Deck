//! Soundboard (#127): one-shot clip playback into a target device.
//!
//! `pw-cat` is spawned and, by default, left to run to completion on its
//! own — this function itself is still fire-and-forget (#393), returning
//! once playback has *started*, not once it finishes. It hands the spawned
//! `Child` back to the caller so a stop mechanism can be layered on top
//! (#399, see `LinuxPipeWireBackend::stop_sound` in `live.rs`) without this
//! module needing to know anything about tracking/lifetime itself.
//! `--target` takes a node name directly, so no id/serial lookup via
//! `pw-dump` is needed the way linking code elsewhere in this module needs
//! one.

use crate::backend::BackendError;
use crate::sysproc;
use std::path::Path;
use std::process::{Child, Stdio};

pub fn play_sound(
    path: &Path,
    target_system_name: &str,
    volume_percent: u8,
) -> Result<Child, BackendError> {
    if !path.is_file() {
        return Err(BackendError::Message(format!(
            "sound file not found: {}",
            path.display()
        )));
    }

    let volume = format!("{:.2}", f32::from(volume_percent.min(100)) / 100.0);

    sysproc::command("pw-cat")
        .args([
            "--playback",
            "--target",
            target_system_name,
            "--volume",
            &volume,
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| BackendError::Message(format!("failed to run pw-cat: {error}")))
}
