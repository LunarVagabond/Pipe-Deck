//! Soundboard (#127) clip listing — reading a user-configured folder of
//! sound files, no PipeWire/backend involvement at all (that's #393's
//! `AudioBackend::play_sound`, wired in by #397).

use serde::{Deserialize, Serialize};
use std::path::Path;

const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "flac", "ogg", "oga", "mp3", "aiff", "aif", "m4a", "opus"];

/// A user-named tab, each backing its own folder of clips — Soundux-style
/// (e.g. "Music", "SFX"), rather than one global folder for the whole
/// soundboard. `id` is generated client-side (`crypto.randomUUID()`, same
/// convention as `Rule.id` — see `Rules.vue`) and passed in on save; the
/// backend never mints one itself.
///
/// Playback destinations (#398) are board-wide, not per-clip: every clip in
/// a tab plays through the same `target`/`monitor` devices at the same
/// volumes. `target` is the mic/virtual-input feed other people or apps
/// hear; `monitor` is a local output (e.g. the user's own speakers) so they
/// can hear/test a clip without it going out to the target at the same
/// level, or without sending it to the target at all. Either leg is
/// independently optional; per-clip overrides were deliberately cut after
/// initial review — mapping destinations per-*tab* instead of per-*board*
/// (i.e. more granular than this) is a possible future ticket, not
/// something to build speculatively now. Both device fields are
/// `system_name`, not a domain device id — the same convention as every
/// other persisted device reference in `config.yaml`
/// (`StreamRouteRule.target_system_name`, `MixSourceSpec.source_system_name`),
/// since a domain id is only stable for one session while `system_name`
/// survives a restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundboardBoard {
    pub id: String,
    pub name: String,
    pub folder: String,
    #[serde(default)]
    pub target_system_name: Option<String>,
    #[serde(default = "default_volume_percent")]
    pub target_volume_percent: u8,
    #[serde(default)]
    pub monitor_system_name: Option<String>,
    #[serde(default = "default_volume_percent")]
    pub monitor_volume_percent: u8,
}

fn default_volume_percent() -> u8 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundboardClip {
    /// The file name (stem + extension) — stable and unique within a single
    /// folder, so it doubles as an id.
    pub id: String,
    pub file_name: String,
    /// File stem, used as the default display label until a user can
    /// rename a clip (not yet — no UI for that exists yet).
    pub label: String,
    pub path: String,
    /// Playback length, probed from the file's own header/metadata (#399,
    /// via `lofty` — cheap, no audio decoding). `None` if the file couldn't
    /// be probed (corrupt/unsupported internals despite a supported
    /// extension) — the frontend falls back to an elapsed-only counter with
    /// no fixed end when this is missing, rather than erroring the whole
    /// clip out.
    #[serde(default)]
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum SoundboardError {
    #[error("soundboard board not found: {0}")]
    BoardNotFound(String),
    #[error("soundboard folder not found: {0}")]
    FolderMissing(String),
    #[error("soundboard path is not a folder: {0}")]
    NotADirectory(String),
}

/// Lists every supported sound file directly inside `folder` (non-recursive
/// — matches how Settings-style folder pickers elsewhere in the app behave,
/// and avoids surprising a user with clips from an unrelated subfolder they
/// happened to nest inside it). Sorted by file name for a stable, predictable
/// display order.
pub fn list_sounds(folder: &Path) -> Result<Vec<SoundboardClip>, SoundboardError> {
    if !folder.exists() {
        return Err(SoundboardError::FolderMissing(folder.display().to_string()));
    }
    if !folder.is_dir() {
        return Err(SoundboardError::NotADirectory(folder.display().to_string()));
    }

    let entries = std::fs::read_dir(folder).map_err(|_| SoundboardError::FolderMissing(folder.display().to_string()))?;

    let mut clips: Vec<SoundboardClip> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let extension = path.extension()?.to_str()?.to_lowercase();
            if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
                return None;
            }
            let file_name = path.file_name()?.to_str()?.to_string();
            let label = path.file_stem()?.to_str()?.to_string();
            let duration_seconds = probe_duration_seconds(&path);
            Some(SoundboardClip {
                id: file_name.clone(),
                file_name,
                label,
                path: path.display().to_string(),
                duration_seconds,
            })
        })
        .collect();

    clips.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(clips)
}

/// Best-effort duration probe via `lofty` (reads the file's own header/
/// metadata, no audio decoding) — `None` on any failure so one unreadable
/// clip never fails the whole folder listing.
fn probe_duration_seconds(path: &Path) -> Option<f64> {
    use lofty::file::AudioFile;

    let tagged_file = lofty::read_from_path(path).ok()?;
    Some(tagged_file.properties().duration().as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pipe-deck-soundboard-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_supported_files_sorted_and_skips_unsupported_ones() {
        let dir = temp_dir("supported");
        std::fs::write(dir.join("b-air-horn.wav"), b"fake").unwrap();
        std::fs::write(dir.join("a-drop.mp3"), b"fake").unwrap();
        std::fs::write(dir.join("notes.txt"), b"not audio").unwrap();
        std::fs::create_dir(dir.join("subfolder")).unwrap();
        std::fs::write(dir.join("subfolder/nested.wav"), b"fake").unwrap();

        let clips = list_sounds(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].file_name, "a-drop.mp3");
        assert_eq!(clips[0].label, "a-drop");
        assert_eq!(clips[1].file_name, "b-air-horn.wav");
        // Neither file has real audio content ("fake" bytes), so probing
        // fails gracefully rather than erroring the listing.
        assert_eq!(clips[0].duration_seconds, None);
    }

    #[test]
    fn missing_folder_is_a_typed_error_not_a_panic() {
        let dir = std::env::temp_dir().join("pipe-deck-soundboard-test-does-not-exist");
        let _ = std::fs::remove_dir_all(&dir);

        let result = list_sounds(&dir);
        assert!(matches!(result, Err(SoundboardError::FolderMissing(_))));
    }

    #[test]
    fn a_file_path_instead_of_a_folder_is_a_typed_error() {
        let dir = temp_dir("not-a-dir");
        let file = dir.join("clip.wav");
        std::fs::write(&file, b"fake").unwrap();

        let result = list_sounds(&file);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(matches!(result, Err(SoundboardError::NotADirectory(_))));
    }

    #[test]
    fn empty_folder_lists_nothing() {
        let dir = temp_dir("empty");
        let clips = list_sounds(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(clips.is_empty());
    }

    /// A minimal, valid 8-bit mono PCM WAV of exactly `seconds` at 8kHz —
    /// enough for `lofty` to read real header/duration data without
    /// depending on any file present on the host system.
    fn make_wav_bytes(seconds: u32) -> Vec<u8> {
        const SAMPLE_RATE: u32 = 8000;
        let data_len = SAMPLE_RATE * seconds;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes()); // byte rate (1 byte/sample)
        bytes.extend_from_slice(&1u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&8u16.to_le_bytes()); // bits per sample
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend(std::iter::repeat_n(128u8, data_len as usize));
        bytes
    }

    #[test]
    fn probes_duration_from_a_real_wav_header() {
        let dir = temp_dir("duration");
        std::fs::write(dir.join("two-seconds.wav"), make_wav_bytes(2)).unwrap();

        let clips = list_sounds(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let duration = clips[0].duration_seconds.expect("a real WAV should yield a probed duration");
        assert!((duration - 2.0).abs() < 0.05, "expected ~2s, got {duration}");
    }
}
