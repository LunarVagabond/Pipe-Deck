use std::path::Path;

use crate::config::ConfigStore;
use crate::core::soundboard;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Plays `path` into `target_device_id`'s underlying device at full
    /// volume (a virtual input or a hardware input passthrough). Fire-and-
    /// forget — see `AudioBackend::play_sound` for what that does and
    /// doesn't guarantee.
    pub fn play_sound(&self, path: &Path, target_device_id: &str) -> Result<(), EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == target_device_id)
            .ok_or_else(|| {
                EngineError::NotFound(format!("device not found: {target_device_id}"))
            })?;

        self.adapter
            .play_sound(path, &device.system_name, 100)
            .map_err(|error| EngineError::Adapter(error.to_string()))
    }

    /// Plays a Soundboard clip (#127) by re-resolving it fresh from config +
    /// disk rather than trusting a client-supplied path: loads the board,
    /// re-lists its folder (so only a file that's actually a direct, still-
    /// present child of the configured folder can ever be played — `clip_id`
    /// crosses the IPC boundary as a plain string), and plays it through the
    /// board's own destinations (#398's `target`/`monitor` — board-wide, not
    /// per-clip).
    ///
    /// A clip can play on either or both of two independent legs — `target`
    /// (what other people/apps hear, e.g. a virtual mic) and `monitor` (a
    /// local output so the user can hear/test the clip themselves) — each
    /// with its own volume. Both are attempted if configured; if both are
    /// configured and one fails, the other still plays and the failure is
    /// still surfaced (not silently swallowed). Errors if the board/clip
    /// doesn't exist, or if the board has neither leg configured yet.
    pub fn play_soundboard_clip(&self, board_id: &str, clip_id: &str) -> Result<(), EngineError> {
        let config = ConfigStore::new()
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let board = config
            .preferences
            .soundboard_boards
            .into_iter()
            .find(|board| board.id == board_id)
            .ok_or_else(|| {
                EngineError::NotFound(format!("soundboard board not found: {board_id}"))
            })?;

        let clips = soundboard::list_sounds(Path::new(&board.folder))
            .map_err(|error| EngineError::InvalidInput(error.to_string()))?;
        let clip = clips
            .into_iter()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| EngineError::NotFound(format!("clip not found: {clip_id}")))?;

        if board.target_system_name.is_none() && board.monitor_system_name.is_none() {
            return Err(EngineError::InvalidInput(format!(
                "\"{}\" tab has no target or monitor device set yet",
                board.name
            )));
        }

        let mut errors = Vec::new();
        if let Some(target) = &board.target_system_name {
            if let Err(error) =
                self.adapter
                    .play_sound(Path::new(&clip.path), target, board.target_volume_percent)
            {
                errors.push(format!("target: {error}"));
            }
        }
        if let Some(monitor) = &board.monitor_system_name {
            if let Err(error) = self.adapter.play_sound(
                Path::new(&clip.path),
                monitor,
                board.monitor_volume_percent,
            ) {
                errors.push(format!("monitor: {error}"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(EngineError::Adapter(errors.join("; ")))
        }
    }

    /// Interrupts whatever Soundboard clip is currently playing (#399) —
    /// thin passthrough to `AudioBackend::stop_sound`, which owns the
    /// actual process handle(s) (see PD-036's rationale for that split).
    pub fn stop_soundboard_clip(&self) -> Result<(), EngineError> {
        self.adapter
            .stop_sound()
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
        assert!(
            clip.is_file(),
            "expected a system test wav to exist at {}",
            clip.display()
        );

        let result = engine.play_sound(clip, &created.device_id);

        let _ = engine.remove_virtual_device(&created.system_name);

        result.expect("play_sound should succeed against a real virtual input");
    }

    #[test]
    #[ignore]
    fn play_soundboard_clip_plays_both_target_and_monitor_legs() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let _guard = crate::config::store::lock_config_dir_env();
        let config_dir = std::env::temp_dir().join("pipe-deck-soundboard-ops-live-test-config");
        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);

        let sounds_dir = std::env::temp_dir().join("pipe-deck-soundboard-ops-live-test-sounds");
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::fs::create_dir_all(&sounds_dir).unwrap();
        std::fs::copy(
            "/usr/share/sounds/speech-dispatcher/test.wav",
            sounds_dir.join("test.wav"),
        )
        .unwrap();

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let target_device = engine
            .create_virtual_input("Pipe Deck Soundboard Clip Play Test Target")
            .expect("create disposable target device");
        let monitor_device = engine
            .create_virtual_input("Pipe Deck Soundboard Clip Play Test Monitor")
            .expect("create disposable monitor device");

        let board = soundboard::SoundboardBoard {
            id: "live-test-board".into(),
            name: "Live Test".into(),
            folder: sounds_dir.display().to_string(),
            target_system_name: Some(target_device.system_name.clone()),
            target_volume_percent: 100,
            monitor_system_name: Some(monitor_device.system_name.clone()),
            monitor_volume_percent: 50,
        };
        ConfigStore::new().ensure_layout().unwrap();
        ConfigStore::new().save_soundboard_board(board).unwrap();

        let result = engine.play_soundboard_clip("live-test-board", "test.wav");

        let _ = engine.remove_virtual_device(&target_device.system_name);
        let _ = engine.remove_virtual_device(&monitor_device.system_name);
        let _ = std::fs::remove_dir_all(&config_dir);
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");

        result.expect("play_soundboard_clip should succeed against real target/monitor devices");
    }

    #[test]
    fn play_soundboard_clip_errors_when_board_has_no_destination_configured() {
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
        engine
            .refresh_graph()
            .expect("mock refresh_graph should not fail");

        let board = soundboard::SoundboardBoard {
            id: "unit-test-board".into(),
            name: "Unit Test".into(),
            folder: sounds_dir.display().to_string(),
            target_system_name: None,
            target_volume_percent: 100,
            monitor_system_name: None,
            monitor_volume_percent: 100,
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
    fn play_soundboard_clip_plays_monitor_only_board_via_mock() {
        let _guard = crate::config::store::lock_config_dir_env();
        let config_dir = std::env::temp_dir().join(format!(
            "pipe-deck-soundboard-ops-unit-test-monitor-only-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
        std::env::set_var("PIPE_DECK_USE_MOCK", "1");

        let sounds_dir = std::env::temp_dir().join(format!(
            "pipe-deck-soundboard-ops-unit-test-monitor-only-sounds-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::fs::create_dir_all(&sounds_dir).unwrap();
        std::fs::write(sounds_dir.join("test-only.wav"), b"fake").unwrap();

        let mut engine = CoreEngine::new();
        engine
            .refresh_graph()
            .expect("mock refresh_graph should not fail");

        let board = soundboard::SoundboardBoard {
            id: "monitor-only-board".into(),
            name: "Monitor Only".into(),
            folder: sounds_dir.display().to_string(),
            target_system_name: None,
            target_volume_percent: 100,
            monitor_system_name: Some("pipe-deck-mock-monitor".to_string()),
            monitor_volume_percent: 60,
        };
        ConfigStore::new().ensure_layout().unwrap();
        ConfigStore::new().save_soundboard_board(board).unwrap();

        let result = engine.play_soundboard_clip("monitor-only-board", "test-only.wav");

        let _ = std::fs::remove_dir_all(&config_dir);
        let _ = std::fs::remove_dir_all(&sounds_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
        std::env::remove_var("PIPE_DECK_USE_MOCK");

        result.expect("a monitor-only board should still play");
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
        engine
            .refresh_graph()
            .expect("mock refresh_graph should not fail");

        let result = engine.play_soundboard_clip("no-such-board", "whatever.wav");

        let _ = std::fs::remove_dir_all(&config_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
        std::env::remove_var("PIPE_DECK_USE_MOCK");

        assert!(matches!(result, Err(EngineError::NotFound(_))));
    }

    #[test]
    fn stop_soundboard_clip_delegates_to_the_adapter() {
        let _guard = crate::config::store::lock_config_dir_env();
        std::env::set_var("PIPE_DECK_USE_MOCK", "1");

        let mut engine = CoreEngine::new();
        engine
            .refresh_graph()
            .expect("mock refresh_graph should not fail");

        let result = engine.stop_soundboard_clip();

        std::env::remove_var("PIPE_DECK_USE_MOCK");

        result.expect("stop_soundboard_clip should succeed against the mock backend");
    }
}
