use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::models::PluginStatus>, String> {
    let engine = state.engine.read().await;
    Ok(engine.list_plugins())
}

#[tauri::command]
pub async fn set_plugin_enabled<R: tauri::Runtime>(
    plugin_id: String,
    enabled: bool,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.set_plugin_enabled(&plugin_id, enabled)?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn grant_plugin_capabilities<R: tauri::Runtime>(
    plugin_id: String,
    capabilities: Vec<String>,
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.grant_plugin_capabilities(&plugin_id, capabilities)?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn list_plugin_ui_panels(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::models::PluginUiPanel>, String> {
    let engine = state.engine.read().await;
    Ok(engine
        .plugin_ui_panels()
        .into_iter()
        .map(|(_, panel)| panel)
        .collect())
}

#[tauri::command]
pub async fn rescan_plugins<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.write().await;
    engine.rescan_plugins()?;
    engine.emit_graph_update(&app);
    Ok(())
}

#[tauri::command]
pub async fn list_plugin_discovery_errors(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::models::PluginDiscoveryIssue>, String> {
    let engine = state.engine.read().await;
    Ok(engine.plugin_discovery_errors())
}

#[tauri::command]
pub async fn list_plugin_routing_suggestions(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::models::RoutingSuggestion>, String> {
    let engine = state.engine.read().await;
    Ok(engine.plugin_routing_suggestions())
}

#[tauri::command]
pub async fn list_plugin_capability_metadata(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::models::CapabilityInfo>, String> {
    let engine = state.engine.read().await;
    Ok(engine.plugin_capability_metadata())
}

#[cfg(test)]
mod tests {
    use super::{
        grant_plugin_capabilities, list_plugin_capability_metadata, list_plugin_discovery_errors,
        list_plugin_routing_suggestions, list_plugin_ui_panels, list_plugins, rescan_plugins,
        set_plugin_enabled,
    };
    use crate::commands::test_support::CommandFixture;
    use crate::config::ConfigStore;
    use crate::core::engine::command_test_with_plugin_manager;
    use crate::core::models::RuntimeGraph;
    use crate::plugins::PluginManager;
    use crate::AppState;
    use serde_json::{json, Value};
    use tauri::{async_runtime::block_on, Manager};

    fn fixture() -> CommandFixture {
        let mut fixture = CommandFixture::new();
        fixture.isolate_plugins();
        fixture
    }

    fn with_plugins(fixture: &CommandFixture, arrange: impl FnOnce(&mut PluginManager)) {
        let state = fixture.app.state::<AppState>();
        command_test_with_plugin_manager(&state.engine.blocking_read(), arrange);
    }

    fn poison_plugins(fixture: &CommandFixture) {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_plugins(fixture, |_| {
                panic!("arranged plugin-manager lock poisoning")
            });
        }))
        .is_err());
    }

    fn manifest(api_version: u32) -> std::path::PathBuf {
        let root = ConfigStore::new().config_dir().join("plugins/inspector-73");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("entry"), b"disabled plugin entry").unwrap();
        let data = json!({"id":"inspector-73","name":"Route Inspector 73","version":"0.4.7","api_version":api_version,
            "entry":"entry","capabilities":["graph.read","ui.panel.register"],"description":"Local route diagnostics","bundled":false,
            "developer":"Audio Lab","repo":"https://example.org/route-inspector"});
        std::fs::write(root.join("plugin.yaml"), serde_json::to_vec(&data).unwrap()).unwrap();
        root
    }

    fn expected_status() -> Value {
        json!([{"id":"inspector-73","name":"Route Inspector 73","version":"0.4.7","description":"Local route diagnostics",
            "bundled":false,"developer":"Audio Lab","repo":"https://example.org/route-inspector","enabled":false,
            "requested_capabilities":["graph.read","ui.panel.register"],"granted_capabilities":[],"runtime_status":"stopped","ui_panels":[]}])
    }

    async fn graph(fixture: &CommandFixture) -> RuntimeGraph {
        fixture
            .app
            .state::<AppState>()
            .engine
            .read()
            .await
            .runtime_graph()
            .clone()
    }

    async fn assert_unchanged_graph_event(fixture: &CommandFixture, before: &RuntimeGraph) {
        let state = fixture.app.state::<AppState>();
        let engine = state.engine.read().await;
        assert_eq!(
            serde_json::to_value(engine.runtime_graph()).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        fixture.assert_graph_update(engine.runtime_graph());
    }

    async fn scan(fixture: &CommandFixture) {
        let before = graph(fixture).await;
        assert_eq!(
            rescan_plugins(
                fixture.app.handle().clone(),
                fixture.app.state::<AppState>()
            )
            .await,
            Ok(())
        );
        assert_unchanged_graph_event(fixture, &before).await;
    }

    #[test]
    fn list_plugins_returns_empty_and_discovered_full_payloads_without_emitting() {
        let fixture = fixture();
        block_on(async {
            assert_eq!(
                serde_json::to_value(list_plugins(fixture.app.state::<AppState>()).await.unwrap())
                    .unwrap(),
                json!([])
            );
            assert!(fixture.take_graph_updates().is_empty());
            manifest(1);
            scan(&fixture).await;
            assert_eq!(
                serde_json::to_value(list_plugins(fixture.app.state::<AppState>()).await.unwrap())
                    .unwrap(),
                expected_status()
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn set_plugin_enabled_disables_the_requested_plugin_and_emits() {
        let fixture = fixture();
        let store = ConfigStore::new();
        let mut config = ConfigStore::default_config();
        config.plugins.insert(
            "inspector-73".into(),
            crate::core::models::PluginEntry {
                enabled: true,
                ..Default::default()
            },
        );
        config.plugins.insert(
            "unrelated-29".into(),
            crate::core::models::PluginEntry {
                enabled: true,
                ..Default::default()
            },
        );
        store.save_config(&config).unwrap();
        block_on(async {
            let before = graph(&fixture).await;
            assert_eq!(
                set_plugin_enabled(
                    "inspector-73".into(),
                    false,
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>()
                )
                .await,
                Ok(())
            );
            assert_eq!(
                serde_json::to_value(store.load_config().unwrap().plugins).unwrap(),
                json!({
                    "inspector-73":{"enabled":false,"granted_capabilities":[],"config":{}},
                    "unrelated-29":{"enabled":true,"granted_capabilities":[],"config":{}}
                })
            );
            assert_unchanged_graph_event(&fixture, &before).await;
        });
    }

    #[test]
    fn set_plugin_enabled_propagates_missing_plugin_without_emitting() {
        let fixture = fixture();
        let error = block_on(set_plugin_enabled(
            "missing-plugin-19".into(),
            true,
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "plugin not found: missing-plugin-19");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn grant_plugin_capabilities_persists_the_requested_grants_and_emits() {
        let fixture = fixture();
        block_on(async {
            let before = graph(&fixture).await;
            assert_eq!(
                grant_plugin_capabilities(
                    "inspector-73".into(),
                    vec!["routing.suggest".into(), "profile.read".into()],
                    fixture.app.handle().clone(),
                    fixture.app.state::<AppState>()
                )
                .await,
                Ok(())
            );
            assert_eq!(
                serde_json::to_value(ConfigStore::new().load_config().unwrap().plugins).unwrap(),
                json!({
                    "inspector-73":{"enabled":false,"granted_capabilities":["routing.suggest","profile.read"],"config":{}}
                })
            );
            assert_unchanged_graph_event(&fixture, &before).await;
        });
    }

    #[test]
    fn grant_plugin_capabilities_propagates_config_error_without_emitting() {
        let fixture = fixture();
        fixture.fail_config_writes();
        let error = block_on(grant_plugin_capabilities(
            "inspector-73".into(),
            vec!["routing.suggest".into()],
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "failed to write config: File exists (os error 17)");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn rescan_plugins_discovers_a_new_plugin_and_emits() {
        let fixture = fixture();
        manifest(1);
        block_on(async {
            scan(&fixture).await;
            assert_eq!(
                serde_json::to_value(list_plugins(fixture.app.state::<AppState>()).await.unwrap())
                    .unwrap(),
                expected_status()
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn rescan_plugins_propagates_poisoned_manager_without_emitting() {
        let fixture = fixture();
        poison_plugins(&fixture);
        let error = block_on(rescan_plugins(
            fixture.app.handle().clone(),
            fixture.app.state::<AppState>(),
        ))
        .unwrap_err();
        assert_eq!(error, "plugin manager lock poisoned");
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_plugin_discovery_errors_returns_empty_and_invalid_manifest_payloads_without_emitting() {
        let fixture = fixture();
        block_on(async {
            assert_eq!(
                serde_json::to_value(
                    list_plugin_discovery_errors(fixture.app.state::<AppState>())
                        .await
                        .unwrap()
                )
                .unwrap(),
                json!([])
            );
            assert!(fixture.take_graph_updates().is_empty());
            let root = manifest(99);
            scan(&fixture).await;
            assert_eq!(
                serde_json::to_value(
                    list_plugin_discovery_errors(fixture.app.state::<AppState>())
                        .await
                        .unwrap()
                )
                .unwrap(),
                json!([{
                    "path":root.display().to_string(),"message":"invalid manifest: unsupported api_version 99 (host supports 1)"
                }])
            );
            assert!(fixture.take_graph_updates().is_empty());
        });
    }

    #[test]
    fn list_plugin_capability_metadata_returns_all_frontend_fields_without_emitting() {
        let fixture = fixture();
        let result = block_on(list_plugin_capability_metadata(
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!([
                {"id":"graph.read","description":"Receive graph.updated notifications","enforced":true},
                {"id":"routing.suggest","description":"Return route suggestions (no apply)","enforced":true},
                {"id":"profile.read","description":"Read active profile metadata","enforced":true},
                {"id":"effects.manage","description":"Manage filter chains on pipe-deck-* devices","enforced":true},
                {"id":"ui.panel.register","description":"Register a nav panel in the host UI","enforced":true}
            ])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_plugin_ui_panels_returns_empty_without_emitting() {
        let fixture = fixture();
        let result = block_on(list_plugin_ui_panels(fixture.app.state::<AppState>())).unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), json!([]));
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn list_plugin_ui_panels_returns_observed_panel_fields_without_emitting() {
        let fixture = fixture();
        with_plugins(&fixture, |manager| {
            crate::plugins::registry::command_test_observe_plugin(
                manager,
                "inspector-73",
                vec![crate::core::models::PluginUiPanel {
                    id: "meter-29".into(),
                    title: "Vocal meter 29".into(),
                    summary: "Post-effects vocal level".into(),
                }],
                vec![],
            )
        });
        let result = block_on(list_plugin_ui_panels(fixture.app.state::<AppState>())).unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!([{"id":"meter-29","title":"Vocal meter 29","summary":"Post-effects vocal level"}])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[test]
    fn list_plugin_routing_suggestions_returns_empty_without_emitting() {
        let fixture = fixture();
        let result = block_on(list_plugin_routing_suggestions(
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), json!([]));
        assert!(fixture.take_graph_updates().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn list_plugin_routing_suggestions_returns_sorted_full_payloads_without_emitting() {
        let fixture = fixture();
        with_plugins(&fixture, |manager| {
            crate::plugins::registry::command_test_observe_plugin(
                manager,
                "inspector-73",
                vec![],
                vec![
                    crate::core::models::RoutingSuggestion {
                        plugin_id: "inspector-73".into(),
                        stream_id: "stream-spotify".into(),
                        target_system_name: "sink-speakers".into(),
                        reason: Some("Music preference".into()),
                        received_at: "2026-01-02T03:04:07Z".into(),
                    },
                    crate::core::models::RoutingSuggestion {
                        plugin_id: "inspector-73".into(),
                        stream_id: "stream-discord".into(),
                        target_system_name: "sink-headphones".into(),
                        reason: None,
                        received_at: "2026-01-02T03:04:05Z".into(),
                    },
                ],
            )
        });
        let result = block_on(list_plugin_routing_suggestions(
            fixture.app.state::<AppState>(),
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!([
                {"plugin_id":"inspector-73","stream_id":"stream-discord","target_system_name":"sink-headphones","received_at":"2026-01-02T03:04:05Z"},
                {"plugin_id":"inspector-73","stream_id":"stream-spotify","target_system_name":"sink-speakers","reason":"Music preference","received_at":"2026-01-02T03:04:07Z"}
            ])
        );
        assert!(fixture.take_graph_updates().is_empty());
    }
}
