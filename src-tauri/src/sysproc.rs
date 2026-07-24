//! Builds [`Command`]s for external system binaries (`pactl`, `pw-dump`,
//! `pw-cli`, `pw-link`, and the `pipe-deck-daemon` sidecar) with the app's
//! own bundle environment stripped off.
//!
//! An AppImage's `AppRun` sets `LD_LIBRARY_PATH` (and `LD_PRELOAD`, when
//! used) to the bundle's own lib directory for every process launched from
//! inside the mount, including child processes we spawn. The daemon sidecar
//! links `libpipewire` directly (issue #148), so the AppImage bundler pulls
//! `libpipewire`/`libspa` into that bundle lib directory as one of its
//! dependencies — which means the *system* `pactl`/`pw-dump`/`pw-link`/
//! `pw-cli` binaries we shell out to, and the daemon sidecar itself, end up
//! dynamically linking against that bundled copy instead of the real system
//! one. A copy built against a different PipeWire install's compiled-in
//! config/module paths fails to create a context at all — surfacing as a
//! confusing `pw_context_new` "No such file or directory" rather than a
//! clean "library not found." These tools must always resolve against the
//! system's own PipeWire/PulseAudio libraries, so every external process we
//! spawn goes through here instead of `Command::new` directly.
use std::ffi::OsStr;
use std::process::Command;

const SCRUB_ENV_VARS: &[&str] = &["LD_LIBRARY_PATH", "LD_PRELOAD"];

pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    for var in SCRUB_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd
}
