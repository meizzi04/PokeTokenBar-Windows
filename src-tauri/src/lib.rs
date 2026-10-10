mod companion;
mod pokeapi;
mod usage;

use companion::{Store, EGG_HATCH_THRESHOLD};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};

const TRAY_ID: &str = "main";
const REFRESH: Duration = Duration::from_secs(60);

type SharedStore = Mutex<Store>;

#[tauri::command]
fn today_usage() -> usage::TodayUsage {
    usage::read_today()
}

#[tauri::command]
fn companion(store: State<SharedStore>) -> Value {
    view(&store.lock().unwrap())
}

/// What the UI needs: saved state plus the thresholds it can't derive.
fn view(store: &Store) -> Value {
    let s = &store.state;
    json!({
        "active": s.active,
        "egg_usage": s.egg_usage,
        "egg_threshold": EGG_HATCH_THRESHOLD,
        "threshold": s.active.as_ref().map(|m| m.threshold()),
        "dex": s.dex,
    })
}

/// Hatches the egg if it's ready. Network calls run without holding the store lock.
fn hatch_if_ready(app: &AppHandle, rng: &mut fastrand::Rng) -> Vec<companion::Event> {
    let store = app.state::<SharedStore>();
    let dir = {
        let s = store.lock().unwrap();
        if !s.state.ready_to_hatch() {
            return vec![];
        }
        s.dir().to_path_buf()
    };
    // Any failure keeps the egg; the next tick retries.
    let Ok(index) = pokeapi::base_index(&dir) else { return vec![] };
    let Some(base) = companion::choose_base(&index, &store.lock().unwrap().state, rng) else {
        return vec![];
    };
    let Ok(line) = pokeapi::line(base) else { return vec![] };
    let mut s = store.lock().unwrap();
    if !s.state.ready_to_hatch() {
        return vec![];
    }
    s.state.hatch(line, rng)
}

fn refresh(app: &AppHandle) {
    let u = usage::read_today();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let label = usage::compact(u.total);
        let _ = tray.set_tooltip(Some(format!("PokeTokenBar · today {label}")));
        // Text next to the icon only renders on macOS; Windows shows the tooltip.
        let _ = tray.set_title(Some(label));
    }

    let today = HashMap::from([
        ("claude".to_string(), u.claude.total()),
        ("codex".to_string(), u.codex.total()),
    ]);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut rng = fastrand::Rng::new();
    let store = app.state::<SharedStore>();
    let mut events = {
        let mut s = store.lock().unwrap();
        let delta = s.state.credit(&today, &date);
        s.state.feed(delta, &mut rng)
    };
    events.extend(hatch_if_ready(app, &mut rng));

    let s = store.lock().unwrap();
    s.save();
    let _ = app.emit("usage", u);
    let _ = app.emit("companion", view(&s));
    for e in events {
        let _ = app.emit("companion-event", e);
    }
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
        .invoke_handler(tauri::generate_handler![today_usage, companion])
        .setup(|app| {
            // Menu bar app like the original: no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage::<SharedStore>(Mutex::new(Store::load(dir.join("companion.json"))));

            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_i, &quit_i])?;

            TrayIconBuilder::with_id(TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "refresh" => {
                        // Off the UI thread: a hatch may hit the network.
                        let app = app.clone();
                        std::thread::spawn(move || refresh(&app));
                    }
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
