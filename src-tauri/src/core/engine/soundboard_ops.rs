use std::path::Path;

use crate::config::ConfigStore;
use crate::core::soundboard;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Plays `path` into `target_device_id`'s underlying device (a virtual
    /// input or a hardware input passthrough). Fire-and-forget — see
    /// `AudioBackend::play_sound` for what that does and doesn't guarantee.
    pub fn play_sound(&self, path: &Path, target_device_id: &str) -> Result<(), EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == target_device_id)
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {target_device_id}")))?;

        self.adapter
            .play_sound(path, &device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))
    }

    /// Plays a Soundboard clip (#127) by re-resolving it fresh from config +
    /// disk rather than trusting a client-supplied path: loads the board,
    /// re-lists its folder (so only a file that's actually a direct, still-
    /// present child of the configured folder can ever be played — `clip_id`
    /// crosses the IPC boundary as a plain string), and looks up its
    /// persisted target `system_name` (#395's `clip_targets`). Errors if the
    /// board/clip doesn't exist or the clip has no target assigned yet.
    pub fn play_soundboard_clip(&self, board_id: &str, clip_id: &str) -> Result<(), EngineError> {
        let config = ConfigStore::new().load_config().map_err(|error| EngineError::Config(error.to_string()))?;
        let board = config
            .preferences
            .soundboard_boards
            .into_iter()
            .find(|board| board.id == board_id)
            .ok_or_else(|| EngineError::NotFound(format!("soundboard board not found: {board_id}")))?;

        let clips = soundboard::list_sounds(Path::new(&board.folder)).map_err(|error| EngineError::InvalidInput(error.to_string()))?;
        let clip = clips
            .into_iter()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| EngineError::NotFound(format!("clip not found: {clip_id}")))?;

        let target = board
            .clip_targets
            .get(clip_id)
            .ok_or_else(|| EngineError::InvalidInput(format!("\"{}\" has no target device set yet", clip.label)))?;

        self.adapter
            .play_sound(Path::new(&clip.path), target)
            .map_err(|error| EngineError::Adapter(error.to_string()))
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as
    //! `virtual_ops::live_tests`. Creates and tears down its own disposable
    //! virtual input.
    use super::*;

    #[test]
    #[ignore]
    fn play_sound_starts_playback_into_a_real_virtual_input() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_input("Pipe Deck Soundboard Playback Test")
            .expect("create disposable test device");

        let clip = Path::new("/usr/share/sounds/speech-dispatcher/test.wav");
        assert!(clip.is_file(), "expected a system test wav to exist at {}", clip.display());

        let result = engine.play_sound(clip, &created.device_id);

        let _ = engine.remove_virtual_device(&created.system_name);

        result.expect("play_sound should succeed against a real virtual input");
    }

    #[test]
    #[ignore]
    fn play_soundboard_clip_resolves_board_and_target_and_plays() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let _guard = crate::config::store::lock_config_dir_env();
        let config_dir = std::env::temp_dir().join("pipe-deck-soundboard-ops-live-test-config");
        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);

        let sounds_dir = std::env::temp_dir().join("pipe-deck-soundboard-ops-live-test-sounds");
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::fs::create_dir_all(&sounds_dir).unwrap();
        std::fs::copy("/usr/share/sounds/speech-dispatcher/test.wav", sounds_dir.join("test.wav")).unwrap();

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_input("Pipe Deck Soundboard Clip Play Test")
            .expect("create disposable test device");

        let mut clip_targets = std::collections::HashMap::new();
        clip_targets.insert("test.wav".to_string(), created.system_name.clone());
        let board = soundboard::SoundboardBoard {
            id: "live-test-board".into(),
            name: "Live Test".into(),
            folder: sounds_dir.display().to_string(),
            clip_targets,
        };
        ConfigStore::new().ensure_layout().unwrap();
        ConfigStore::new().save_soundboard_board(board).unwrap();

        let result = engine.play_soundboard_clip("live-test-board", "test.wav");

        let _ = engine.remove_virtual_device(&created.system_name);
        let _ = std::fs::remove_dir_all(&config_dir);
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");

        result.expect("play_soundboard_clip should succeed against a real board/clip/target");
    }

    #[test]
    fn play_soundboard_clip_errors_when_clip_has_no_target_assigned() {
        let _guard = crate::config::store::lock_config_dir_env();
        let config_dir = std::env::temp_dir().join(format!(
            "pipe-deck-soundboard-ops-unit-test-config-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
        std::env::set_var("PIPE_DECK_USE_MOCK", "1");

        let sounds_dir = std::env::temp_dir().join(format!(
            "pipe-deck-soundboard-ops-unit-test-sounds-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::fs::create_dir_all(&sounds_dir).unwrap();
        std::fs::write(sounds_dir.join("untargeted.wav"), b"fake").unwrap();

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("mock refresh_graph should not fail");

        let board = soundboard::SoundboardBoard {
            id: "unit-test-board".into(),
            name: "Unit Test".into(),
            folder: sounds_dir.display().to_string(),
            clip_targets: std::collections::HashMap::new(),
        };
        ConfigStore::new().ensure_layout().unwrap();
        ConfigStore::new().save_soundboard_board(board).unwrap();

        let result = engine.play_soundboard_clip("unit-test-board", "untargeted.wav");

        let _ = std::fs::remove_dir_all(&config_dir);
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
        std::env::remove_var("PIPE_DECK_USE_MOCK");

        assert!(matches!(result, Err(EngineError::InvalidInput(_))));
    }

    #[test]
    fn play_soundboard_clip_errors_for_an_unknown_board() {
        let _guard = crate::config::store::lock_config_dir_env();
        let config_dir = std::env::temp_dir().join(format!(
            "pipe-deck-soundboard-ops-unit-test-unknown-board-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
        std::env::set_var("PIPE_DECK_USE_MOCK", "1");

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("mock refresh_graph should not fail");

        let result = engine.play_soundboard_clip("no-such-board", "whatever.wav");

        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
        std::env::remove_var("PIPE_DECK_USE_MOCK");

        assert!(matches!(result, Err(EngineError::NotFound(_))));
    }
}
