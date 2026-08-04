use crate::config::ConfigStore;
use crate::core::soundboard::{self, SoundboardBoard, SoundboardClip, SoundboardError};
use std::path::PathBuf;

#[tauri::command]
pub fn list_soundboard_boards() -> Vec<SoundboardBoard> {
    ConfigStore::new().load_config().map(|config| config.preferences.soundboard_boards).unwrap_or_default()
}

#[tauri::command]
pub fn save_soundboard_board(board: SoundboardBoard) -> Result<(), String> {
    ConfigStore::new().save_soundboard_board(board).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_soundboard_board(board_id: String) -> Result<(), String> {
    ConfigStore::new().delete_soundboard_board(&board_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_soundboard_sounds(board_id: String) -> Result<Vec<SoundboardClip>, String> {
    let config = ConfigStore::new().load_config().map_err(|error| error.to_string())?;
    let board = config
        .preferences
        .soundboard_boards
        .into_iter()
        .find(|board| board.id == board_id)
        .ok_or_else(|| SoundboardError::BoardNotFound(board_id).to_string())?;

    soundboard::list_sounds(&PathBuf::from(board.folder)).map_err(|error| error.to_string())
}
