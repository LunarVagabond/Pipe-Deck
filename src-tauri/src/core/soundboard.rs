//! Soundboard (#127) clip listing — reading a user-configured folder of
//! sound files, no PipeWire/backend involvement at all (that's #393's
//! `AudioBackend::play_sound`, wired in by #397).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "flac", "ogg", "oga", "mp3", "aiff", "aif", "m4a", "opus"];

/// A user-named tab, each backing its own folder of clips — Soundux-style
/// (e.g. "Music", "SFX"), rather than one global folder for the whole
/// soundboard. `id` is generated client-side (`crypto.randomUUID()`, same
/// convention as `Rule.id` — see `Rules.vue`) and passed in on save; the
/// backend never mints one itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundboardBoard {
    pub id: String,
    pub name: String,
    pub folder: String,
    /// `SoundboardClip.id` (a file name, unique within this board's own
    /// folder — never global) → target device `system_name`. `system_name`
    /// rather than a domain device id, matching how every other persisted
    /// device reference in `config.yaml` is keyed (`StreamRouteRule.target_system_name`,
    /// `MixSourceSpec.source_system_name`) — a domain id is only stable for
    /// one session; `CoreEngine.device_id_remap` exists specifically to
    /// paper over it changing across a device recreation, which a
    /// persisted-to-disk reference can't rely on. #397 is what actually
    /// reads this to drive playback; this ticket only persists it.
    #[serde(default)]
    pub clip_targets: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundboardClip {
    /// The file name (stem + extension) — stable and unique within a single
    /// folder, so it doubles as an id and as the key into
    /// `SoundboardBoard.clip_targets`.
    pub id: String,
    pub file_name: String,
    /// File stem, used as the default display label until a user can
    /// rename a clip (not yet — no UI for that exists before #398).
    pub label: String,
    pub path: String,
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
            Some(SoundboardClip {
                id: file_name.clone(),
                file_name,
                label,
                path: path.display().to_string(),
            })
        })
        .collect();

    clips.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(clips)
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
}
