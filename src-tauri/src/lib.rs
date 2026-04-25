use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf, thread, time::Duration};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

const MAIN_LABEL: &str = "main";
const SETTINGS_LABEL: &str = "settings";
const ABOUT_LABEL: &str = "about";
const DEFAULT_RIGHT_OFFSET: i32 = 10;
const DEFAULT_BOTTOM_OFFSET: i32 = 10;
static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum LaunchMode {
    Tray,
    Window,
}

fn default_startup_launch_mode() -> LaunchMode {
    LaunchMode::Tray
}

fn default_manual_launch_mode() -> LaunchMode {
    LaunchMode::Window
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    #[serde(default = "default_right_offset")]
    window_right_offset: i32,
    #[serde(default = "default_bottom_offset")]
    window_bottom_offset: i32,
    #[serde(default = "default_startup_launch_mode")]
    startup_launch_mode: LaunchMode,
    #[serde(default = "default_manual_launch_mode")]
    manual_launch_mode: LaunchMode,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            window_right_offset: DEFAULT_RIGHT_OFFSET,
            window_bottom_offset: DEFAULT_BOTTOM_OFFSET,
            startup_launch_mode: default_startup_launch_mode(),
            manual_launch_mode: default_manual_launch_mode(),
        }
    }
}

fn default_right_offset() -> i32 {
    DEFAULT_RIGHT_OFFSET
}

fn default_bottom_offset() -> i32 {
    DEFAULT_BOTTOM_OFFSET
}

#[tauri::command]
fn get_settings() -> AppSettings {
    load_settings().unwrap_or_default()
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<(), String> {
    store_settings(&settings).map_err(|error| error.to_string())
}

#[tauri::command]
fn dismiss_main_window(app: AppHandle) {
    dismiss_window(&app, MAIN_LABEL);
}

#[tauri::command]
fn dismiss_after_launch(app: AppHandle, app_name: String) {
    println!("launch placeholder: {app_name}");
    dismiss_window(&app, MAIN_LABEL);
}

#[tauri::command]
fn close_settings_window(app: AppHandle) {
    dismiss_window(&app, SETTINGS_LABEL);
}

fn is_startup_launch() -> bool {
    std::env::args().any(|arg| arg == "--startup" || arg == "--autostart")
}

fn should_show_main_window_on_launch(settings: &AppSettings) -> bool {
    if is_startup_launch() {
        settings.startup_launch_mode == LaunchMode::Window
    } else {
        settings.manual_launch_mode == LaunchMode::Window
    }
}

fn settings_path() -> io::Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;

    Ok(PathBuf::from(local_app_data)
        .join("com.fengqi.taurilaunch")
        .join("settings.json"))
}

fn load_settings() -> io::Result<AppSettings> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(path)?;
    let settings = serde_json::from_str(&content).unwrap_or_default();
    Ok(settings)
}

fn store_settings(settings: &AppSettings) -> io::Result<()> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(settings)?;
    fs::write(path, content)
}

fn lightweight_mode_enabled() -> bool {
    LIGHTWEIGHT_MODE.load(Ordering::Relaxed)
}

fn dismiss_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        dismiss_webview_window(&window);
    }
}

fn dismiss_webview_window(window: &WebviewWindow) {
    if lightweight_mode_enabled() {
        let _ = window.destroy();
    } else {
        let _ = window.hide();
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        position_main_window(&window);
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    match WebviewWindowBuilder::new(app, MAIN_LABEL, WebviewUrl::App("index.html".into()))
        .title("应用列表")
        .inner_size(720.0, 470.0)
        .min_inner_size(620.0, 360.0)
        .resizable(false)
        .decorations(false)
        .visible(false)
        .build()
    {
        Ok(window) => {
            attach_main_window_events(window.clone());
            position_main_window(&window);
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("failed to create main window: {error}"),
    }
}

fn position_main_window(window: &WebviewWindow) {
    let settings = load_settings().unwrap_or_default();
    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };

    let Ok(size) = window.outer_size() else {
        return;
    };

    let work_area = monitor.work_area();
    let x = work_area.position.x
        + work_area.size.width as i32
        - size.width as i32
        - settings.window_right_offset.max(0);
    let y = work_area.position.y
        + work_area.size.height as i32
        - size.height as i32
        - settings.window_bottom_offset.max(0);

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_settings_window(app: &AppHandle) {
    show_aux_window(app, SETTINGS_LABEL, "设置", "index.html?view=settings", 500.0, 320.0);
}

fn show_about_window(app: &AppHandle) {
    show_aux_window(app, ABOUT_LABEL, "关于", "index.html?view=about", 360.0, 260.0);
}

fn show_aux_window(
    app: &AppHandle,
    label: &str,
    title: &str,
    url: &str,
    width: f64,
    height: f64,
) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    match WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(width, height)
        .resizable(false)
        .decorations(true)
        .center()
        .build()
    {
        Ok(window) => {
            attach_aux_window_events(window.clone(), label == SETTINGS_LABEL);
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("failed to create {label} window: {error}"),
    }
}

fn attach_main_window_events(window: WebviewWindow) {
    let close_window = window.clone();
    let was_focused = Arc::new(AtomicBool::new(false));
    let focus_close_armed = Arc::new(AtomicBool::new(false));
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            dismiss_webview_window(&close_window);
        }
        WindowEvent::Focused(true) => {
            was_focused.store(true, Ordering::Relaxed);
            let focus_close_armed = focus_close_armed.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(800));
                focus_close_armed.store(true, Ordering::Relaxed);
            });
        }
        WindowEvent::Focused(false) => {
            if was_focused.load(Ordering::Relaxed) && focus_close_armed.load(Ordering::Relaxed) {
                dismiss_webview_window(&close_window);
            }
        }
        _ => {}
    });
}

fn attach_aux_window_events(window: WebviewWindow, use_lightweight_mode: bool) {
    let close_window = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if use_lightweight_mode {
                dismiss_webview_window(&close_window);
            } else {
                let _ = close_window.destroy();
            }
        }
    });
}

fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let about = MenuItem::with_id(app, "about", "关于", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "扫描", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let lightweight =
        CheckMenuItem::with_id(app, "lightweight", "轻量模式", true, false, None::<&str>)?;
    let menu = Menu::with_items(app, &[&about, &scan, &settings, &lightweight, &quit])?;
    let lightweight_for_event = lightweight.clone();

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("TauriLaunch")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "about" => show_about_window(app),
            "scan" => {
                println!("scan placeholder");
            }
            "settings" => show_settings_window(app),
            "lightweight" => {
                let checked = lightweight_for_event.is_checked().unwrap_or(false);
                LIGHTWEIGHT_MODE.store(checked, Ordering::Relaxed);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            create_tray(app)?;
            let settings = load_settings().unwrap_or_default();
            if should_show_main_window_on_launch(&settings) {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dismiss_main_window,
            dismiss_after_launch,
            get_settings,
            save_settings,
            close_settings_window
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|_app, event| {
        if let RunEvent::ExitRequested { code, api, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
