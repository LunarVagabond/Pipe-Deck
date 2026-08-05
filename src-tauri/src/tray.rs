use crate::config::ConfigStore;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray-show", "Show Pipe Deck", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "tray-hide", "Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &quit])?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::FailedToReceiveMessage)?
        .clone();

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Pipe Deck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-show" => show_main_window(app),
            "tray-hide" => hide_main_window(app),
            "tray-quit" => app.exit(0),
            _ => {}
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

    Ok(())
}

/// Close (X) behavior is user-configurable (#295): "minimize" hides the
/// window (today's long-standing default), "quit" exits the process, and
/// an unset preference (first launch, or an install that predates #295)
/// means the user hasn't been asked yet. `api.prevent_close()` always runs
/// first regardless of branch — the "quit" case still exits deliberately
/// via `app.exit(0)` rather than letting the close proceed, and the unset
/// case needs the window to stay open under the prompt.
pub fn attach_close_to_tray(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();

                let behavior = ConfigStore::new()
                    .load_config()
                    .ok()
                    .and_then(|config| config.preferences.close_behavior);

                match behavior.as_deref() {
                    Some("quit") => app_handle.exit(0),
                    Some("minimize") => hide_main_window(&app_handle),
                    _ => {
                        let _ = app_handle.emit("close-behavior-prompt-needed", ());
                    }
                }
            }
        });
    }
}

/// Persists via `set_close_behavior` are the answer to the one-time prompt
/// as well as a settings change, so this performs the action for whichever
/// close click prompted the choice (or, from Settings, is a no-op window
/// action since the window is already visible/open).
pub(crate) fn apply_close_behavior(app: &tauri::AppHandle, behavior: &str) {
    if behavior == "quit" {
        app.exit(0);
    } else {
        hide_main_window(app);
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
