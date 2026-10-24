use crate::config::ConfigStore;
use crate::core::soundboard::{self, SoundboardBoard, SoundboardClip, SoundboardError};
use crate::AppState;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn list_soundboard_boards() -> Vec<SoundboardBoard> {
    ConfigStore::new()
        .load_config()
        .map(|config| config.preferences.soundboard_boards)
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_soundboard_board(board: SoundboardBoard) -> Result<(), String> {
    ConfigStore::new()
        .save_soundboard_board(board)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_soundboard_board(board_id: String) -> Result<(), String> {
    ConfigStore::new()
        .delete_soundboard_board(&board_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_soundboard_sounds(board_id: String) -> Result<Vec<SoundboardClip>, String> {
    let config = ConfigStore::new()
        .load_config()
        .map_err(|error| error.to_string())?;
    let board = config
        .preferences
        .soundboard_boards
        .into_iter()
        .find(|board| board.id == board_id)
        .ok_or_else(|| SoundboardError::BoardNotFound(board_id).to_string())?;

    soundboard::list_sounds(&PathBuf::from(board.folder)).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn play_soundboard_clip(
    board_id: String,
    clip_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let engine = state.engine.read().await;
    engine
        .play_soundboard_clip(&board_id, &clip_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn stop_soundboard_clip(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.engine.read().await;
    engine
        .stop_soundboard_clip()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mock::{CommandTestSoundboard, MockAudioBackend};
    use crate::commands::test_support::CommandFixture;
    use std::sync::{Arc, Mutex};
    use tauri::{async_runtime::block_on, Manager};

    fn board() -> SoundboardBoard {
        SoundboardBoard {
            id: "desk-board".into(),
            name: "Desk clips".into(),
            folder: ConfigStore::new()
                .config_dir()
                .join("clips")
                .display()
                .to_string(),
            target_system_name: Some("source-mic-filtered".into()),
            target_volume_percent: 31,
            monitor_system_name: Some("sink-speakers".into()),
            monitor_volume_percent: 67,
            exclusive_playback: false,
        }
    }

    fn other_board() -> SoundboardBoard {
        SoundboardBoard {
            id: "other-board".into(),
            name: "Other clips".into(),
            ..board()
        }
    }

    fn seed_clips() -> PathBuf {
        let folder = PathBuf::from(board().folder);
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("b-horn.wav"), b"invalid audio header").unwrap();
        std::fs::write(folder.join("a-bell.mp3"), b"invalid audio header").unwrap();
        std::fs::write(folder.join("notes.txt"), b"not audio").unwrap();
        std::fs::create_dir(folder.join("nested")).unwrap();
        std::fs::write(folder.join("nested/hidden.wav"), b"invalid audio header").unwrap();
        ConfigStore::new().save_soundboard_board(board()).unwrap();
        folder
    }

    fn observe_playback(fixture: &CommandFixture) -> Arc<Mutex<CommandTestSoundboard>> {
        let observed = Arc::new(Mutex::new(CommandTestSoundboard::default()));
        let mut backend = MockAudioBackend::new();
        backend.command_test_soundboard = Some(Arc::clone(&observed));
        fixture.use_backend(Box::new(backend));
        observed
    }

    #[test]
    fn list_boards_returns_empty_and_full_saved_boards_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(list_soundboard_boards(), vec![]);
        assert!(fixture.take_graph_updates().is_empty());
        ConfigStore::new().save_soundboard_board(board()).unwrap();
        ConfigStore::new()
            .save_soundboard_board(other_board())
            .unwrap();
        assert_eq!(list_soundboard_boards(), vec![board(), other_board()]);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_boards_returns_empty_on_read_failure_without_emitting() {
        let fixture = CommandFixture::new();
        std::fs::create_dir(ConfigStore::new().config_dir().join("config.yaml")).unwrap();
        assert_eq!(list_soundboard_boards(), vec![]);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_board_inserts_and_updates_only_the_requested_board_without_emitting() {
        let fixture = CommandFixture::new();
        ConfigStore::new()
            .save_soundboard_board(other_board())
            .unwrap();
        assert_eq!(save_soundboard_board(board()), Ok(()));
        assert_eq!(
            ConfigStore::new()
                .load_config()
                .unwrap()
                .preferences
                .soundboard_boards,
            vec![other_board(), board()]
        );
        assert!(fixture.take_graph_updates().is_empty());
        let updated = SoundboardBoard {
            name: "Renamed clips".into(),
            target_volume_percent: 49,
            monitor_system_name: None,
            exclusive_playback: true,
            ..board()
        };
        assert_eq!(save_soundboard_board(updated.clone()), Ok(()));
        assert_eq!(
            ConfigStore::new()
                .load_config()
                .unwrap()
                .preferences
                .soundboard_boards,
            vec![other_board(), updated]
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_board_maps_write_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            save_soundboard_board(board()),
            Err("failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn delete_board_removes_only_the_requested_board_without_emitting() {
        let fixture = CommandFixture::new();
        ConfigStore::new().save_soundboard_board(board()).unwrap();
        ConfigStore::new()
            .save_soundboard_board(other_board())
            .unwrap();
        assert_eq!(delete_soundboard_board("desk-board".into()), Ok(()));
        assert_eq!(
            ConfigStore::new()
                .load_config()
                .unwrap()
                .preferences
                .soundboard_boards,
            vec![other_board()]
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn delete_board_maps_write_error_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            delete_soundboard_board("desk-board".into()),
            Err("failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_sounds_returns_every_clip_field_in_filename_order_without_emitting() {
        let fixture = CommandFixture::new();
        let folder = seed_clips();
        assert_eq!(
            list_soundboard_sounds("desk-board".into()).unwrap(),
            vec![
                SoundboardClip {
                    id: "a-bell.mp3".into(),
                    file_name: "a-bell.mp3".into(),
                    label: "a-bell".into(),
                    path: folder.join("a-bell.mp3").display().to_string(),
                    duration_seconds: None
                },
                SoundboardClip {
                    id: "b-horn.wav".into(),
                    file_name: "b-horn.wav".into(),
                    label: "b-horn".into(),
                    path: folder.join("b-horn.wav").display().to_string(),
                    duration_seconds: None
                },
            ]
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_sounds_maps_missing_board_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            list_soundboard_sounds("absent-board".into()),
            Err("soundboard board not found: absent-board".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn play_clip_forwards_both_destinations_and_distinct_volumes_without_emitting() {
        let fixture = CommandFixture::new();
        let folder = seed_clips();
        let observed = observe_playback(&fixture);
        assert_eq!(
            block_on(play_soundboard_clip(
                "desk-board".into(),
                "b-horn.wav".into(),
                fixture.app.state::<AppState>()
            )),
            Ok(())
        );
        let observed = observed.lock().unwrap();
        assert_eq!(
            observed.played,
            vec![
                (folder.join("b-horn.wav"), "source-mic-filtered".into(), 31),
                (folder.join("b-horn.wav"), "sink-speakers".into(), 67),
            ]
        );
        assert_eq!(observed.stops, 0);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn play_clip_maps_missing_board_without_emitting() {
        let fixture = CommandFixture::new();
        let observed = observe_playback(&fixture);
        assert_eq!(
            block_on(play_soundboard_clip(
                "absent-board".into(),
                "b-horn.wav".into(),
                fixture.app.state::<AppState>()
            )),
            Err("not found: soundboard board not found: absent-board".into())
        );
        assert!(observed.lock().unwrap().played.is_empty());
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn stop_clip_reaches_the_backend_without_emitting() {
        let fixture = CommandFixture::new();
        let observed = observe_playback(&fixture);
        assert_eq!(
            block_on(stop_soundboard_clip(fixture.app.state::<AppState>())),
            Ok(())
        );
        let observed = observed.lock().unwrap();
        assert_eq!(observed.stops, 1);
        assert!(observed.played.is_empty());
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn stop_clip_maps_backend_error_without_emitting() {
        let fixture = CommandFixture::new();
        let observed = observe_playback(&fixture);
        observed.lock().unwrap().stop_error = Some("playback handle unavailable".into());
        assert_eq!(
            block_on(stop_soundboard_clip(fixture.app.state::<AppState>())),
            Err("pipewire adapter error: playback handle unavailable".into())
        );
        assert_eq!(observed.lock().unwrap().stops, 1);
        assert!(fixture.take_graph_updates().is_empty());
    }
}
