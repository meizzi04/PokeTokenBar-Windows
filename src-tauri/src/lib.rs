mod usage;

use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

const TRAY_ID: &str = "main";
const REFRESH: Duration = Duration::from_secs(60);

#[tauri::command]
fn today_usage() -> usage::TodayUsage {
    usage::read_today()
}

fn refresh(app: &AppHandle) {
    let u = usage::read_today();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let label = usage::compact(u.total);
        let _ = tray.set_tooltip(Some(format!("PokeTokenBar · today {label}")));
        // Text next to the icon only renders on macOS; Windows shows the tooltip.
        let _ = tray.set_title(Some(label));
    }
    let _ = app.emit("usage", u);
}

fn toggle_window(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
    } else {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![today_usage])
        .setup(|app| {
            // Menu bar app like the original: no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_i, &quit_i])?;

            TrayIconBuilder::with_id(TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "refresh" => refresh(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        toggle_window(tray.app_handle());
                    }
                })
                .build(app)?;

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                refresh(&handle);
                std::thread::sleep(REFRESH);
            });
            Ok(())
        })
        .on_window_event(|w, e| {
            // Closing the popover hides it; the app lives in the tray.
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
