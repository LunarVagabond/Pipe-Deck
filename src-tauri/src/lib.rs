pub mod backend;
pub mod commands;
pub mod config;
pub mod core;
pub mod daemon;
pub mod pipewire;
pub mod plugins;
pub mod tray;

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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::app_info::open_url,
            commands::graph::get_runtime_graph,
            commands::config::get_config,
            commands::config::get_config_paths,
            commands::config::list_profiles,
            commands::config::set_device_alias,
            commands::config::set_show_system_streams,
            commands::config::set_auto_apply_rules,
            commands::config::set_sidebar_collapsed,
            commands::config::list_themes,
            commands::config::set_theme_mode,
            commands::config::set_dark_scheme,
            commands::config::set_light_scheme,
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
            commands::routing::clear_stream_target,
            commands::routing::undo_last_routing,
            commands::routing::can_undo_routing,
            commands::routing::get_last_error,
            commands::rules::list_rules,
            commands::rules::save_rule,
            commands::rules::delete_rule,
            commands::rules::toggle_rule,
            commands::rules::simulate_rules,
            commands::rules::apply_rules,
            commands::mixer::set_device_volume,
            commands::mixer::set_device_mute,
            commands::mixer::set_stream_volume,
            commands::mixer::set_stream_mute,
            commands::effects::set_device_effects,
            commands::effects::get_effect_chains,
            commands::effects::get_effect_capabilities,
            commands::effects::preflight_effect_chain,
            commands::effects::is_effect_chain_live,
            commands::effects::apply_effect_chain_structural,
            commands::effects::remove_effect_chain_structural,
            commands::effects::set_effect_chain_live_params,
            commands::virtual_device::create_virtual_output,
            commands::virtual_device::create_virtual_multi_output,
            commands::virtual_device::create_virtual_input,
            commands::virtual_device::remove_virtual_device,
            commands::virtual_device::set_virtual_mic_mix,
            commands::virtual_device::add_mix_source,
            commands::virtual_device::remove_mix_source,
            commands::virtual_device::set_mix_source_volume,
            commands::virtual_device::set_mix_source_mute,
            commands::virtual_device::enable_stream_mic_passthrough,
            commands::daemon::get_daemon_status,
            commands::daemon::enable_background_restore,
            commands::daemon::disable_background_restore,
            commands::daemon::set_restore_on_startup,
            commands::plugins::list_plugins,
            commands::plugins::set_plugin_enabled,
            commands::plugins::grant_plugin_capabilities,
            commands::plugins::list_plugin_ui_panels,
            commands::plugins::rescan_plugins,
            commands::plugins::list_plugin_discovery_errors,
            commands::plugins::list_plugin_capability_metadata,
            commands::plugins::list_plugin_routing_suggestions,
        ])
        .setup(|app| {
            tray::setup_tray(app)?;
            tray::attach_close_to_tray(&app.handle());

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
