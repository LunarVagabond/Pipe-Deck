use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Listener, Manager,
};

use crate::core::engine::CoreEngine;
use crate::AppState;

/// Menu id prefix for a "switch default output to this device" submenu
/// entry — the device id is appended, e.g. `tray-set-default-output::abc123`.
const DEFAULT_OUTPUT_PREFIX: &str = "tray-set-default-output::";
const MUTE_TOGGLE_ID: &str = "tray-mute-toggle";

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();

    // Built with placeholder/disabled dynamic entries — the real default
    // output and mute state aren't known yet at this point in startup (the
    // core engine's first `refresh_graph` hasn't run). Corrected within
    // moments by the `rebuild_tray_menu` spawned below, and kept in sync
    // afterwards by the `graph-updated` listener (#11).
    let menu = build_menu_items(handle, None)?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::FailedToReceiveMessage)?
        .clone();

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Pipe Deck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            match id {
                "tray-show" => show_main_window(app),
                "tray-hide" => hide_main_window(app),
                "tray-quit" => app.exit(0),
                MUTE_TOGGLE_ID => spawn_toggle_mute(app),
                other => {
                    if let Some(device_id) = other.strip_prefix(DEFAULT_OUTPUT_PREFIX) {
                        spawn_set_default_output(app, device_id.to_string());
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        show_main_window(app);
                    }
                }
            }
        })
        .build(app)?;

    // Stashed as managed state so `rebuild_tray_menu` (run from a menu-event
    // handler or the `graph-updated` listener below, neither of which has
    // the local `tray` this closure captured at build time) can find it
    // again via `AppHandle::state`.
    app.manage(tray);

    // Tray quick controls must work with the main window closed (#11's
    // acceptance criteria), and the mute checkbox must reflect live
    // PipeWire state within one refresh cycle rather than a separate poll
    // — so the tray subscribes to the exact same `graph-updated` event the
    // frontend does (`core/engine/graph_sync.rs::emit_graph_update`)
    // instead of talking to the webview at all.
    let listener_handle = handle.clone();
    app.listen("graph-updated", move |_event| {
        let handle = listener_handle.clone();
        tauri::async_runtime::spawn(async move {
            let _ = rebuild_tray_menu(&handle).await;
        });
    });

    // First real population, once `CoreEngine::initialize` (spawned
    // separately in `lib.rs`) has had a chance to run.
    let initial_handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let _ = rebuild_tray_menu(&initial_handle).await;
    });

    Ok(())
}

pub fn attach_close_to_tray(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let window_handle = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window_handle.hide();
                api.prevent_close();
            }
        });
    }
}

pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// Rebuilds the tray menu from the engine's current graph state and swaps
/// it in via `TrayIcon::set_menu` — called on every `graph-updated` event
/// (mixer/routing changes from the UI, rule automation, live PipeWire
/// changes picked up by the graph watcher, and tray actions themselves)
/// so the default-output submenu and mute checkbox never go stale.
async fn rebuild_tray_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    let menu = {
        let engine = state.engine.read().await;
        build_menu_items(app, Some(&engine))?
    };
    if let Some(tray) = app.try_state::<TrayIcon<tauri::Wry>>() {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

/// Builds the full tray menu. `engine` is `None` only for the very first,
/// placeholder build in [`setup_tray`], before the core engine has fetched
/// its first graph snapshot.
fn build_menu_items(app: &tauri::AppHandle, engine: Option<&CoreEngine>) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "tray-show", "Show Pipe Deck", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "tray-hide", "Hide", true, None::<&str>)?;
    let top_separator = PredefinedMenuItem::separator(app)?;

    let default_device = engine.and_then(|engine| engine.default_output_device());
    let default_output_label = MenuItem::with_id(
        app,
        "tray-default-output-label",
        format!(
            "Output: {}",
            default_device.map(|device| device.label.as_str()).unwrap_or("Unknown")
        ),
        false,
        None::<&str>,
    )?;

    let outputs = engine.map(|engine| engine.available_output_devices()).unwrap_or_default();
    let output_submenu = Submenu::with_id(app, "tray-output-submenu", "Switch Output", !outputs.is_empty())?;
    if outputs.is_empty() {
        let placeholder = MenuItem::with_id(app, "tray-output-none", "No output devices available", false, None::<&str>)?;
        output_submenu.append(&placeholder)?;
    } else {
        for device in outputs {
            let checked = default_device.is_some_and(|default| default.id == device.id);
            let item = CheckMenuItem::with_id(
                app,
                format!("{DEFAULT_OUTPUT_PREFIX}{}", device.id),
                device.label.clone(),
                true,
                checked,
                None::<&str>,
            )?;
            output_submenu.append(&item)?;
        }
    }

    let mute_checked = engine.and_then(|engine| engine.default_output_muted()).unwrap_or(false);
    let mute_item = CheckMenuItem::with_id(
        app,
        MUTE_TOGGLE_ID,
        "Mute",
        default_device.is_some(),
        mute_checked,
        None::<&str>,
    )?;

    let bottom_separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &show,
            &hide,
            &top_separator,
            &default_output_label,
            &output_submenu,
            &mute_item,
            &bottom_separator,
            &quit,
        ],
    )
}

fn spawn_toggle_mute(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut engine = state.engine.write().await;
        if let Err(error) = engine.toggle_default_output_mute() {
            eprintln!("tray mute toggle failed: {error}");
            return;
        }
        engine.emit_graph_update(&app);
    });
}

fn spawn_set_default_output(app: &tauri::AppHandle, device_id: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut engine = state.engine.write().await;
        if let Err(error) = engine.set_default_output_device(&device_id) {
            eprintln!("tray set default output failed: {error}");
            return;
        }
        engine.emit_graph_update(&app);
    });
}
