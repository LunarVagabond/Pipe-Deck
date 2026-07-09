use crate::core::models::{ApplyResult, Profile, ProfileIndexEntry};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_profile(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<Profile, String> {
    let engine = state.engine.read().await;
    engine.get_profile(&profile_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_profile(
    profile_id: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<Profile, String> {
    let mut engine = state.engine.write().await;
    engine
        .save_profile(&profile_id, name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_profile_as(
    profile_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Profile, String> {
    let mut engine = state.engine.write().await;
    engine
        .save_profile_as(&profile_id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn import_profile(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<ProfileIndexEntry, String> {
    let engine = state.engine.read().await;
    engine
        .import_profile(&source_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn import_profile_archive(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<ProfileIndexEntry, String> {
    let engine = state.engine.read().await;
    engine
        .import_profile_archive(&source_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn export_profile(
    profile_id: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let engine = state.engine.read().await;
    engine
        .export_profile(&profile_id, &destination)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_profile_drift(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<crate::core::models::RoutingDrift, String> {
    let engine = state.engine.read().await;
    engine
        .get_profile_drift(&profile_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn apply_profile_routes(
    profile_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .apply_profile_routes(&profile_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[tauri::command]
pub async fn swap_profile(
    profile_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .swap_profile(&profile_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}
