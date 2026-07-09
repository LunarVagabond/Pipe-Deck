pub mod commands;
pub mod config;
pub mod core;
pub mod pipewire;

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
