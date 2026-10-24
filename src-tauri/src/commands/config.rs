use crate::config::{ConfigStore, ProfileStore, ThemeStore};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub fn get_config() -> crate::core::models::AppConfig {
    ConfigStore::new()
        .load_config()
        .unwrap_or_else(|_| ConfigStore::default_config())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPaths {
    config_dir: String,
    profiles_dir: String,
    plugins_dir: String,
}

#[tauri::command]
pub fn get_config_paths() -> ConfigPaths {
    let config_store = ConfigStore::new();
    let profiles_dir = ProfileStore::new(config_store.config_dir().clone()).profiles_dir();
    let plugins_dir = crate::plugins::registry::user_plugins_dir();
    ConfigPaths {
        config_dir: config_store.config_dir().display().to_string(),
        profiles_dir: profiles_dir.display().to_string(),
        plugins_dir: plugins_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn list_profiles() -> Vec<crate::core::models::ProfileIndexEntry> {
    ConfigStore::new().list_profiles().unwrap_or_default()
}

#[tauri::command]
pub async fn set_device_alias<R: tauri::Runtime>(
    system_name: String,
    alias: String,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine
        .apply_device_alias(&system_name, &alias)
        .map_err(|error| error.to_string())?;
    engine.refresh_graph().map_err(|error| error.to_string())?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub fn set_show_system_streams(show: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_show_system_streams(show)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_auto_apply_rules(enabled: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_auto_apply_rules(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_sidebar_collapsed(collapsed: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_sidebar_collapsed(collapsed)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_onboarding_dismissed(dismissed: bool) -> Result<(), String> {
    ConfigStore::new()
        .set_onboarding_dismissed(dismissed)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_themes() -> Vec<crate::core::models::ResolvedScheme> {
    let config_store = ConfigStore::new();
    ThemeStore::new(config_store.config_dir().clone()).list_schemes()
}

#[tauri::command]
pub fn set_theme_mode(mode: String) -> Result<(), String> {
    ConfigStore::new()
        .set_theme_mode(&mode)
        .map_err(|error| error.to_string())
}

/// Persists the user's close-button choice. `apply_now` distinguishes the
/// two callers: the one-time prompt (#295) answers *for* a click that's
/// already in flight, so it passes `true` to also perform the hide/exit
/// immediately — otherwise the window is left in limbo (not hidden, not
/// quit). A plain preference change from Settings passes `false`, since no
/// close is in progress there.
#[tauri::command]
pub fn set_close_behavior(
    behavior: String,
    apply_now: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    ConfigStore::new()
        .set_close_behavior(&behavior)
        .map_err(|error| error.to_string())?;
    if apply_now {
        crate::tray::apply_close_behavior(&app, &behavior);
    }
    Ok(())
}

#[tauri::command]
pub fn set_dark_scheme(id: String) -> Result<(), String> {
    ConfigStore::new()
        .set_dark_scheme(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_light_scheme(id: String) -> Result<(), String> {
    ConfigStore::new()
        .set_light_scheme(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_notice_duration_ms(ms: u32) -> Result<(), String> {
    ConfigStore::new()
        .set_notice_duration_ms(ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_update_channel(channel: String) -> Result<(), String> {
    ConfigStore::new()
        .set_update_channel(&channel)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::CommandFixture;
    use serde_json::{json, Value};
    use tauri::{async_runtime::block_on, Manager};

    fn default_payload() -> Value {
        json!({
            "version": 1, "active_profile": "default",
            "profile_index": [{"id": "default", "name": "Default", "file": "profiles/default.yaml"}],
            "devices": {}, "rules": [], "routing_rules": {"stream_rules": []},
            "virtual_devices": [], "processing_nodes": [], "plugins": {},
            "preferences": {
                "show_system_streams": false, "restore_on_startup": true,
                "background_restore": false, "auto_apply_rules": true,
                "sidebar_collapsed": false, "onboarding_dismissed": false,
                "theme_mode": "dark", "dark_scheme": "midnight-deck", "light_scheme": "paper-deck",
                "notice_duration_ms": 5000, "soundboard_boards": [], "close_behavior": null,
                "update_channel": "latest"
            }
        })
    }

    fn assert_config(expected: Value) {
        assert_eq!(
            serde_json::to_value(ConfigStore::new().load_config().unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn get_config_returns_defaults_and_the_complete_saved_config() {
        let fixture = CommandFixture::new();
        assert_eq!(
            serde_json::to_value(get_config()).unwrap(),
            default_payload()
        );
        let mut expected = default_payload();
        expected["preferences"]["theme_mode"] = json!("system");
        expected["devices"] = json!({"source-mic": {"alias": "Desk microphone"}});
        ConfigStore::new()
            .save_config(&serde_json::from_value(expected.clone()).unwrap())
            .unwrap();
        assert_eq!(serde_json::to_value(get_config()).unwrap(), expected);
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_config_falls_back_when_config_cannot_be_read() {
        let fixture = CommandFixture::new();
        std::fs::create_dir(ConfigStore::new().config_dir().join("config.yaml")).unwrap();
        assert_eq!(
            serde_json::to_value(get_config()).unwrap(),
            default_payload()
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn get_config_paths_returns_all_three_frontend_paths() {
        let fixture = CommandFixture::new();
        let root = std::env::var("PIPE_DECK_CONFIG_DIR").unwrap();
        assert_eq!(
            serde_json::to_value(get_config_paths()).unwrap(),
            json!({
                "configDir": root, "profilesDir": format!("{root}/profiles"),
                "pluginsDir": format!("{root}/plugins")
            })
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_profiles_returns_the_complete_saved_index() {
        let fixture = CommandFixture::new();
        let mut config = ConfigStore::default_config();
        config.profile_index = vec![crate::core::models::ProfileIndexEntry {
            id: "studio".into(),
            name: "Studio desk".into(),
            file: "profiles/custom.yaml".into(),
        }];
        ConfigStore::new().save_config(&config).unwrap();
        assert_eq!(
            serde_json::to_value(list_profiles()).unwrap(),
            json!([
                {"id": "studio", "name": "Studio desk", "file": "profiles/custom.yaml"}
            ])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_profiles_returns_empty_on_read_failure() {
        let fixture = CommandFixture::new();
        std::fs::create_dir(ConfigStore::new().config_dir().join("config.yaml")).unwrap();
        assert!(list_profiles().is_empty());
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn set_device_alias_persists_refreshes_and_emits() {
        let fixture = CommandFixture::new();
        assert_eq!(
            block_on(set_device_alias(
                "source-mic".into(),
                "Desk microphone".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )),
            Ok(())
        );
        let mut expected = default_payload();
        expected["devices"] = json!({"source-mic": {"alias": "Desk microphone"}});
        assert_config(expected);
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.blocking_read();
        let mic = engine
            .runtime_graph()
            .devices
            .iter()
            .find(|device| device.id == "source-mic")
            .unwrap();
        assert_eq!(mic.system_name, "source-mic");
        assert_eq!(mic.label, "Desk microphone");
        fixture.assert_graph_update(engine.runtime_graph());
    }

    #[test]
    fn set_device_alias_maps_write_failure_without_emitting() {
        let fixture = CommandFixture::new();
        fixture.fail_config_writes();
        assert_eq!(
            block_on(set_device_alias(
                "source-mic".into(),
                "Desk microphone".into(),
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>(),
            )),
            Err("config error: failed to write config: File exists (os error 17)".into())
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    macro_rules! preference_tests {
        ($happy:ident, $error:ident, $command:ident, $field:literal, $first:expr, $second:expr) => {
            #[test]
            fn $happy() {
                let fixture = CommandFixture::new();
                for (value, expected_value) in [($first, json!($first)), ($second, json!($second))]
                {
                    assert_eq!($command(value), Ok(()));
                    let mut expected = default_payload();
                    expected["preferences"][$field] = expected_value;
                    assert_config(expected);
                    assert!(fixture.take_graph_updates().is_empty());
                }
            }

            #[test]
            fn $error() {
                let fixture = CommandFixture::new();
                fixture.fail_config_writes();
                assert_eq!(
                    $command($first),
                    Err("failed to write config: File exists (os error 17)".into())
                );
                assert!(fixture.take_graph_updates().is_empty());
            }
        };
    }

    preference_tests!(
        show_streams_persists_both_states,
        show_streams_maps_write_error,
        set_show_system_streams,
        "show_system_streams",
        true,
        false
    );
    preference_tests!(
        auto_rules_persists_both_states,
        auto_rules_maps_write_error,
        set_auto_apply_rules,
        "auto_apply_rules",
        false,
        true
    );
    preference_tests!(
        sidebar_persists_both_states,
        sidebar_maps_write_error,
        set_sidebar_collapsed,
        "sidebar_collapsed",
        true,
        false
    );
    preference_tests!(
        onboarding_persists_both_states,
        onboarding_maps_write_error,
        set_onboarding_dismissed,
        "onboarding_dismissed",
        true,
        false
    );
    preference_tests!(
        theme_mode_persists_both_values,
        theme_mode_maps_write_error,
        set_theme_mode,
        "theme_mode",
        String::from("system"),
        String::from("light")
    );
    preference_tests!(
        dark_scheme_persists_both_values,
        dark_scheme_maps_write_error,
        set_dark_scheme,
        "dark_scheme",
        String::from("copper-dusk"),
        String::from("custom:night")
    );
    preference_tests!(
        light_scheme_persists_both_values,
        light_scheme_maps_write_error,
        set_light_scheme,
        "light_scheme",
        String::from("meadow-light"),
        String::from("custom:day")
    );
    preference_tests!(
        notice_duration_persists_both_values,
        notice_duration_maps_write_error,
        set_notice_duration_ms,
        "notice_duration_ms",
        1732_u32,
        0_u32
    );
    preference_tests!(
        update_channel_persists_both_values,
        update_channel_maps_write_error,
        set_update_channel,
        "update_channel",
        String::from("prerelease"),
        String::from("latest")
    );

    #[test]
    fn list_themes_returns_full_builtin_and_custom_palettes() {
        let fixture = CommandFixture::new();
        let themes = ConfigStore::new().config_dir().join("themes");
        std::fs::create_dir(&themes).unwrap();
        std::fs::write(
            themes.join("desk.yaml"),
            "name: Desk colors\nbase: dark\ncolors:\n  accent_teal: '#123456'\n",
        )
        .unwrap();
        let palettes = [
            (
                "midnight-deck",
                "Midnight Deck",
                "dark",
                "builtin",
                [
                    "#0b0f14", "#131820", "#1c2330", "#2a3344", "#e6e9ef", "#9aa4b2", "#7c5cff",
                    "#26c3a3", "#ffb020", "#34d399", "#fb923c", "#f87171",
                ],
            ),
            (
                "copper-dusk",
                "Copper Dusk",
                "dark",
                "builtin",
                [
                    "#12100e", "#1b1815", "#26211c", "#3a322a", "#f0e9e0", "#b0a394", "#e0794b",
                    "#4bb0a0", "#f2c14e", "#34d399", "#fb923c", "#f87171",
                ],
            ),
            (
                "paper-deck",
                "Paper Deck",
                "light",
                "builtin",
                [
                    "#f5f6f8", "#ffffff", "#eef0f4", "#d3d8e0", "#1a2230", "#5b6675", "#6b47e6",
                    "#12a08a", "#d98a00", "#34d399", "#fb923c", "#f87171",
                ],
            ),
            (
                "meadow-light",
                "Meadow Light",
                "light",
                "builtin",
                [
                    "#f4f7f2", "#ffffff", "#e8efe6", "#cdd8c8", "#1c2a1e", "#5d6b5a", "#2f8f5b",
                    "#0e9488", "#e08a1e", "#34d399", "#fb923c", "#f87171",
                ],
            ),
            (
                "custom:desk",
                "Desk colors",
                "dark",
                "custom",
                [
                    "#0b0f14", "#131820", "#1c2330", "#2a3344", "#e6e9ef", "#9aa4b2", "#7c5cff",
                    "#123456", "#ffb020", "#34d399", "#fb923c", "#f87171",
                ],
            ),
        ];
        let expected: Vec<Value> = palettes.into_iter().map(|(id, name, kind, source, c)| json!({
            "id": id, "name": name, "kind": kind, "source": source,
            "colors": {"background": c[0], "surface_1": c[1], "surface_2": c[2], "border": c[3],
                "text": c[4], "text_muted": c[5], "accent_purple": c[6], "accent_teal": c[7],
                "accent_amber": c[8], "status_success": c[9], "status_warning": c[10], "status_danger": c[11]}
        })).collect();
        assert_eq!(
            serde_json::to_value(list_themes()).unwrap(),
            json!(expected)
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
