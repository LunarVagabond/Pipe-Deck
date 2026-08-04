use crate::config::ConfigStore;
use crate::core::soundboard::{self, SoundboardClip, SoundboardError};
use std::path::PathBuf;

#[tauri::command]
pub fn get_soundboard_folder() -> Option<String> {
    ConfigStore::new().load_config().ok()?.preferences.soundboard_folder
}

#[tauri::command]
pub fn set_soundboard_folder(folder: String) -> Result<(), String> {
    ConfigStore::new().set_soundboard_folder(&folder).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_soundboard_sounds() -> Result<Vec<SoundboardClip>, String> {
    let folder = ConfigStore::new()
        .load_config()
        .map_err(|error| error.to_string())?
        .preferences
        .soundboard_folder
        .ok_or(SoundboardError::NotConfigured)
        .map_err(|error| error.to_string())?;

    soundboard::list_sounds(&PathBuf::from(folder)).map_err(|error| error.to_string())
}
