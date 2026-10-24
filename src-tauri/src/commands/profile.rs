use crate::core::models::{ApplyResult, Profile, ProfileIndexEntry};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_profile(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<Profile, String> {
    let engine = state.engine.read().await;
    engine
        .get_profile(&profile_id)
        .map_err(|error| error.to_string())
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
pub async fn delete_profile(profile_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .delete_profile(&profile_id)
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
pub async fn apply_profile_routes<R: tauri::Runtime>(
    profile_id: String,
    app: tauri::AppHandle<R>,
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
pub async fn swap_profile<R: tauri::Runtime>(
    profile_id: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<ApplyResult, String> {
    let mut engine = state.engine.write().await;
    let result = engine
        .swap_profile(&profile_id)
        .map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::CommandFixture;
    use crate::config::{ConfigStore, ProfileStore};
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};
    use std::path::Path;
    use tauri::{async_runtime::block_on, Manager};

    fn profile() -> Profile {
        serde_json::from_value(json!({
            "version": 1, "id": "studio", "name": "Studio desk",
            "created": "2025-01-02T03:04:05Z", "updated": "2025-06-07T08:09:10Z",
            "routing_intents": [{"stream_id": "stream-discord", "target_device_id": "sink-speakers", "target_system_name": "sink-speakers"}],
            "volume_state": {"sink-speakers": {"volume_percent": 37, "muted": true, "system_name": "sink-speakers"}},
            "device_assumptions": {}, "effect_state": {}
        })).unwrap()
    }

    fn seed_profile() {
        let store = ConfigStore::new();
        store.ensure_layout().unwrap();
        let entry = ProfileStore::new(store.config_dir().clone())
            .save_profile_as("studio", "Studio desk", &profile())
            .unwrap();
        store.add_profile_to_index(entry).unwrap();
    }

    fn assert_profile_file(expected: &Profile) {
        let path = ConfigStore::new().config_dir().join("profiles/studio.yaml");
        let saved: Profile = serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(&saved, expected);
    }

    fn capture_graph(fixture: &CommandFixture) {
        fixture.use_backend(Box::new(crate::backend::stub::StubBackend::new()));
        let state = fixture.app.state::<AppState>();
        let mut engine = state.engine.blocking_write();
        let mut graph = engine.runtime_graph().clone();
        graph.devices.retain(|device| device.id == "sink-speakers");
        graph.streams.retain(|stream| stream.id == "stream-discord");
        graph.streams[0].current_target = Some("sink-speakers".into());
        graph.links.clear();
        engine.apply_graph_update(graph);
    }

    fn assert_capture(result: &Profile, name: &str, created: Option<&str>, before: DateTime<Utc>) {
        let updated = DateTime::parse_from_rfc3339(&result.updated)
            .unwrap()
            .with_timezone(&Utc);
        assert!(updated >= before && updated <= Utc::now());
        if let Some(created) = created {
            assert_eq!(result.created, created);
        } else {
            let created = DateTime::parse_from_rfc3339(&result.created)
                .unwrap()
                .with_timezone(&Utc);
            assert!(created >= before && created <= updated);
        }
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({
                "version": 1, "id": "studio", "name": name,
                "created": result.created, "updated": result.updated,
                "routing_intents": [{"stream_id": "stream-discord", "target_device_id": "sink-speakers", "target_system_name": "sink-speakers"}],
                "volume_state": {"sink-speakers": {"volume_percent": 70, "muted": false, "system_name": "sink-speakers"}},
                "device_assumptions": {}, "effect_state": {}
            })
        );
        assert_profile_file(result);
    }

    fn make_archive(path: &Path, included_profile: Option<&Profile>) {
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::default(),
        );
        let mut archive = tar::Builder::new(encoder);
        if let Some(profile) = included_profile {
            let yaml = serde_yaml::to_string(profile).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(yaml.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, "profile.yaml", yaml.as_bytes())
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn get_profile_returns_the_complete_profile_without_emitting() {
        let fixture = CommandFixture::new();
        seed_profile();
        assert_eq!(
            block_on(get_profile(
                "studio".into(),
                fixture.app.state::<AppState>()
            ))
            .unwrap(),
            profile()
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_profile_maps_missing_profile_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(get_profile(
                "absent-profile".into(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile not found: absent-profile"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_profile_captures_state_preserves_creation_and_updates_the_name() {
        let fixture = CommandFixture::new();
        seed_profile();
        capture_graph(&fixture);
        let before = Utc::now();
        let result = block_on(save_profile(
            "studio".into(),
            Some("Renamed studio".into()),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_capture(
            &result,
            "Renamed studio",
            Some("2025-01-02T03:04:05Z"),
            before,
        );
        assert_eq!(
            serde_json::to_value(ConfigStore::new().list_profiles().unwrap()).unwrap(),
            json!([
                {"id": "default", "name": "Default", "file": "profiles/default.yaml"},
                {"id": "studio", "name": "Renamed studio", "file": "profiles/studio.yaml"}
            ])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_profile_maps_missing_profile_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(save_profile(
                "absent-profile".into(),
                None,
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile not found: absent-profile"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn delete_profile_removes_only_the_requested_file_and_index_entry() {
        let fixture = CommandFixture::new();
        seed_profile();
        assert_eq!(
            block_on(delete_profile(
                "studio".into(),
                fixture.app.state::<AppState>()
            )),
            Ok(())
        );
        assert!(!ConfigStore::new()
            .config_dir()
            .join("profiles/studio.yaml")
            .exists());
        assert!(ConfigStore::new()
            .config_dir()
            .join("profiles/default.yaml")
            .is_file());
        assert_eq!(
            serde_json::to_value(ConfigStore::new().list_profiles().unwrap()).unwrap(),
            json!([
                {"id": "default", "name": "Default", "file": "profiles/default.yaml"}
            ])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn delete_profile_rejects_the_active_profile_without_emitting() {
        let fixture = CommandFixture::new();
        seed_profile();
        assert_eq!(
            block_on(delete_profile(
                "default".into(),
                fixture.app.state::<AppState>()
            )),
            Err(
                "invalid input: cannot delete the active profile; swap to another profile first"
                    .into()
            )
        );
        assert!(ConfigStore::new()
            .config_dir()
            .join("profiles/default.yaml")
            .is_file());
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_profile_as_captures_the_complete_graph_and_adds_an_index_entry() {
        let fixture = CommandFixture::new();
        capture_graph(&fixture);
        let before = Utc::now();
        let result = block_on(save_profile_as(
            "studio".into(),
            "New studio".into(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_capture(&result, "New studio", None, before);
        assert_eq!(
            serde_json::to_value(ConfigStore::new().list_profiles().unwrap()).unwrap(),
            json!([
                {"id": "default", "name": "Default", "file": "profiles/default.yaml"},
                {"id": "studio", "name": "New studio", "file": "profiles/studio.yaml"}
            ])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn save_profile_as_maps_validation_error_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(save_profile_as(
                "studio".into(),
                " ".into(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile validation failed: profile name is required"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn import_profile_returns_the_full_index_entry_and_persists_the_profile() {
        let fixture = CommandFixture::new();
        let path = ConfigStore::new().config_dir().join("incoming.yaml");
        std::fs::write(&path, serde_yaml::to_string(&profile()).unwrap()).unwrap();
        let result = block_on(import_profile(
            path.display().to_string(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"id": "studio", "name": "Studio desk", "file": "profiles/studio.yaml"})
        );
        assert_profile_file(&profile());
        assert_eq!(ConfigStore::new().list_profiles().unwrap().len(), 2);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn import_profile_maps_validation_error_without_emitting() {
        let fixture = CommandFixture::new();
        let path = ConfigStore::new().config_dir().join("incoming.yaml");
        let mut invalid = profile();
        invalid.version = 0;
        std::fs::write(&path, serde_yaml::to_string(&invalid).unwrap()).unwrap();
        assert_eq!(
            block_on(import_profile(
                path.display().to_string(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile validation failed: version must be >= 1"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn import_profile_archive_returns_the_full_entry_and_persists_the_profile() {
        let fixture = CommandFixture::new();
        let path = ConfigStore::new().config_dir().join("incoming.tar.gz");
        make_archive(&path, Some(&profile()));
        let result = block_on(import_profile_archive(
            path.display().to_string(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"id": "studio", "name": "Studio desk", "file": "profiles/studio.yaml"})
        );
        assert_profile_file(&profile());
        assert_eq!(ConfigStore::new().list_profiles().unwrap().len(), 2);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn import_profile_archive_maps_missing_payload_without_emitting() {
        let fixture = CommandFixture::new();
        let path = ConfigStore::new().config_dir().join("incoming.tar.gz");
        make_archive(&path, None);
        assert_eq!(
            block_on(import_profile_archive(
                path.display().to_string(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: failed to read profile: archive missing profile.yaml"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn export_profile_writes_the_complete_profile_and_manifest_without_emitting() {
        use std::io::Read;
        let fixture = CommandFixture::new();
        seed_profile();
        let destination = ConfigStore::new().config_dir().join("export.tar.gz");
        let before = Utc::now();
        assert_eq!(
            block_on(export_profile(
                "studio".into(),
                destination.display().to_string(),
                fixture.app.state::<AppState>()
            )),
            Ok(())
        );
        let decoder = flate2::read::GzDecoder::new(std::fs::File::open(destination).unwrap());
        let mut archive = tar::Archive::new(decoder);
        let mut contents = std::collections::BTreeMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().display().to_string();
            let mut yaml = String::new();
            entry.read_to_string(&mut yaml).unwrap();
            assert!(contents.insert(path, yaml).is_none());
        }
        assert_eq!(
            contents.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["manifest.yaml", "profile.yaml"]
        );
        assert_eq!(
            serde_yaml::from_str::<Profile>(&contents["profile.yaml"]).unwrap(),
            profile()
        );
        let manifest: Value = serde_yaml::from_str(&contents["manifest.yaml"]).unwrap();
        let exported = DateTime::parse_from_rfc3339(manifest["exported_at"].as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        assert!(exported >= before && exported <= Utc::now());
        assert_eq!(
            manifest,
            json!({"version": 1, "exported_at": manifest["exported_at"], "profile_id": "studio", "profile_name": "Studio desk"})
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn export_profile_maps_missing_profile_without_emitting() {
        let fixture = CommandFixture::new();
        let destination = ConfigStore::new().config_dir().join("export.tar.gz");
        assert_eq!(
            block_on(export_profile(
                "absent-profile".into(),
                destination.display().to_string(),
                fixture.app.state::<AppState>()
            )),
            Err("profile error: profile not found: absent-profile".into())
        );
        assert!(!destination.exists());
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_profile_drift_returns_the_complete_difference_without_emitting() {
        let fixture = CommandFixture::new();
        seed_profile();
        let result = block_on(get_profile_drift(
            "studio".into(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({
                "profile_id": "studio", "profile_name": "Studio desk", "has_drift": true,
                "items": [{"stream_id": "stream-discord", "stream_label": "Discord", "live_target_id": "sink-chat",
                    "live_target_label": "Chat", "desired_target_id": "sink-speakers", "desired_target_label": "Speakers"}]
            })
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_profile_drift_maps_missing_profile_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(get_profile_drift(
                "absent-profile".into(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile not found: absent-profile"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    fn assert_applied_graph(fixture: &CommandFixture, volume: u8, muted: bool) {
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.blocking_read();
        let graph = engine.runtime_graph();
        let targets: Vec<_> = graph
            .streams
            .iter()
            .map(|stream| (stream.id.as_str(), stream.current_target.as_deref()))
            .collect();
        assert_eq!(
            targets,
            vec![
                ("stream-discord", Some("sink-speakers")),
                ("stream-spotify", None),
                ("stream-steam", None),
                ("stream-firefox", None),
                ("stream-obs", None)
            ]
        );
        let speakers = graph
            .devices
            .iter()
            .find(|device| device.id == "sink-speakers")
            .unwrap();
        assert_eq!(
            (speakers.volume_percent, speakers.muted),
            (Some(volume), Some(muted))
        );
        assert!(engine.can_undo_routing());
        fixture.assert_graph_update(graph);
    }

    #[test]
    fn apply_profile_routes_applies_only_routing_and_emits() {
        let fixture = CommandFixture::new();
        seed_profile();
        let result = block_on(apply_profile_routes(
            "studio".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": true, "message": null})
        );
        assert_eq!(
            ConfigStore::new()
                .load_config()
                .unwrap()
                .active_profile
                .as_deref(),
            Some("default")
        );
        assert_applied_graph(&fixture, 70, false);
    }

    #[test]
    fn apply_profile_routes_maps_missing_profile_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(apply_profile_routes(
                "absent-profile".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile not found: absent-profile"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn swap_profile_applies_routing_and_volume_sets_active_and_emits() {
        let fixture = CommandFixture::new();
        seed_profile();
        let result = block_on(swap_profile(
            "studio".into(),
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"success": true, "message": null})
        );
        assert_eq!(
            ConfigStore::new()
                .load_config()
                .unwrap()
                .active_profile
                .as_deref(),
            Some("studio")
        );
        assert_applied_graph(&fixture, 37, true);
    }

    #[test]
    fn swap_profile_maps_missing_profile_without_emitting() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(swap_profile(
                "absent-profile".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            ))
            .unwrap_err(),
            "profile error: profile not found: absent-profile"
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
