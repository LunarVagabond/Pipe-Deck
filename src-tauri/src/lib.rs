pub mod commands;
pub mod config;
pub mod core;
pub mod daemon;
pub mod pipewire;
pub mod plugins;

use core::engine::CoreEngine;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

pub struct AppState {
    pub engine: Arc<RwLock<CoreEngine>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = CoreEngine::new();
    let state = AppState {
        engine: Arc::new(RwLock::new(engine)),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::graph::get_runtime_graph,
            commands::config::get_config,
            commands::config::list_profiles,
            commands::config::set_device_alias,
            commands::config::set_show_system_streams,
            commands::profile::get_profile,
            commands::profile::save_profile,
            commands::profile::save_profile_as,
            commands::profile::import_profile,
            commands::profile::import_profile_archive,
            commands::profile::export_profile,
            commands::profile::get_profile_drift,
            commands::profile::apply_profile_routes,
            commands::profile::swap_profile,
            commands::routing::set_stream_target,
            commands::routing::set_stream_targets,
            commands::routing::set_device_route,
            commands::routing::set_device_targets,
            commands::routing::undo_last_routing,
            commands::routing::can_undo_routing,
            commands::routing::get_last_error,
            commands::rules::list_rules,
            commands::rules::save_rule,
            commands::rules::delete_rule,
            commands::rules::toggle_rule,
            commands::rules::simulate_rules,
            commands::mixer::set_device_volume,
            commands::mixer::set_device_mute,
            commands::virtual_device::create_virtual_output,
            commands::virtual_device::create_virtual_multi_output,
            commands::virtual_device::create_virtual_input,
            commands::virtual_device::remove_virtual_device,
            commands::daemon::get_daemon_status,
            commands::daemon::enable_background_restore,
            commands::daemon::disable_background_restore,
            commands::daemon::set_restore_on_startup,
            commands::plugins::list_plugins,
            commands::plugins::set_plugin_enabled,
            commands::plugins::grant_plugin_capabilities,
            commands::plugins::list_plugin_ui_panels,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let engine_arc = app.state::<AppState>().engine.clone();

            tauri::async_runtime::spawn(async move {
                let engine_for_sub = engine_arc.clone();
                let mut engine = engine_arc.write().await;
                if let Err(error) = engine.initialize(&handle, engine_for_sub).await {
                    eprintln!("failed to start core engine: {error}");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
