use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, RunEvent, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

const MAIN_LABEL: &str = "main";
const SETTINGS_LABEL: &str = "settings";
const ABOUT_LABEL: &str = "about";
const DEFAULT_RIGHT_OFFSET: i32 = 10;
const DEFAULT_BOTTOM_OFFSET: i32 = 10;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
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

fn default_watched_directories() -> Vec<String> {
    let user_profile =
        std::env::var("USERPROFILE").unwrap_or_else(|_| String::from("C:\\Users\\fengqi"));
    ["App", "Game", "SingleExe"]
        .iter()
        .map(|name| {
            PathBuf::from(&user_profile)
                .join("Desktop")
                .join(name)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
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
    #[serde(default = "default_watched_directories")]
    watched_directories: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            window_right_offset: DEFAULT_RIGHT_OFFSET,
            window_bottom_offset: DEFAULT_BOTTOM_OFFSET,
            startup_launch_mode: default_startup_launch_mode(),
            manual_launch_mode: default_manual_launch_mode(),
            watched_directories: default_watched_directories(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppEntry {
    id: String,
    name: String,
    path: String,
    launch_args: String,
    working_dir: String,
    launches: u32,
    accent: String,
    initials: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShortcutInfo {
    target_path: String,
    arguments: String,
    working_directory: String,
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
fn get_apps() -> Vec<AppEntry> {
    load_apps().unwrap_or_default()
}

#[tauri::command]
fn scan_apps(app: AppHandle) -> Result<Vec<AppEntry>, String> {
    scan_store_and_emit(&app).map_err(|error| error.to_string())
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

fn app_data_dir() -> io::Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;

    Ok(PathBuf::from(local_app_data).join("com.fengqi.taurilaunch"))
}

fn settings_path() -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join("settings.json"))
}

fn apps_path() -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join("apps.json"))
}

fn load_settings() -> io::Result<AppSettings> {
    let path = settings_path()?;
    if !path.exists() {
        let settings = AppSettings::default();
        store_settings(&settings)?;
        return Ok(settings);
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

fn load_apps() -> io::Result<Vec<AppEntry>> {
    let path = apps_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)?;
    let apps = serde_json::from_str(&content).unwrap_or_default();
    Ok(apps)
}

fn store_apps(apps: &[AppEntry]) -> io::Result<()> {
    let path = apps_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(apps)?;
    fs::write(path, content)
}

fn scan_store_and_emit(app: &AppHandle) -> io::Result<Vec<AppEntry>> {
    let apps = scan_configured_apps()?;
    store_apps(&apps)?;
    let _ = app.emit("apps-updated", apps.clone());
    Ok(apps)
}

fn scan_configured_apps() -> io::Result<Vec<AppEntry>> {
    let settings = load_settings()?;
    let previous_launches: HashMap<String, u32> = load_apps()?
        .into_iter()
        .map(|app| (app.id, app.launches))
        .collect();
    let mut apps = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for directory in settings.watched_directories {
        let root = PathBuf::from(directory);
        if !root.is_dir() {
            continue;
        }
        scan_directory(&root, &previous_launches, &mut seen, &mut apps)?;
    }

    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
    });
    Ok(apps)
}

fn scan_directory(
    root: &Path,
    previous_launches: &HashMap<String, u32>,
    seen: &mut HashMap<String, ()>,
    apps: &mut Vec<AppEntry>,
) -> io::Result<()> {
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                pending.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_lowercase();

            let app_entry = match extension.as_str() {
                "exe" => Some(app_entry_from_exe(&path)),
                "lnk" => app_entry_from_shortcut(&path),
                _ => None,
            };

            if let Some(mut app_entry) = app_entry {
                if app_entry.path.is_empty() {
                    continue;
                }

                let dedupe_key = format!(
                    "{}\n{}\n{}",
                    app_entry.path.to_lowercase(),
                    app_entry.launch_args,
                    app_entry.source.to_lowercase()
                );

                if seen.insert(dedupe_key, ()).is_some() {
                    continue;
                }

                app_entry.launches = previous_launches
                    .get(&app_entry.id)
                    .copied()
                    .unwrap_or_default();
                apps.push(app_entry);
            }
        }
    }

    Ok(())
}

fn app_entry_from_exe(path: &Path) -> AppEntry {
    let path_text = path.to_string_lossy().into_owned();
    let name = file_stem(path);
    build_app_entry(
        name,
        path_text.clone(),
        String::new(),
        parent_dir(path),
        path_text,
    )
}

fn app_entry_from_shortcut(path: &Path) -> Option<AppEntry> {
    let shortcut = read_shortcut(path)?;
    let target_path = shortcut.target_path.trim().to_string();
    if target_path.is_empty() {
        return None;
    }

    let name = file_stem(path);
    Some(build_app_entry(
        name,
        target_path,
        shortcut.arguments.trim().to_string(),
        shortcut.working_directory.trim().to_string(),
        path.to_string_lossy().into_owned(),
    ))
}

fn read_shortcut(path: &Path) -> Option<ShortcutInfo> {
    let shortcut_path = powershell_single_quoted(path);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::UTF8
$shortcutPath = {shortcut_path}
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcutPath)
[pscustomobject]@{{
  TargetPath = $shortcut.TargetPath
  Arguments = $shortcut.Arguments
  WorkingDirectory = $shortcut.WorkingDirectory
}} | ConvertTo-Json -Compress
"#
    );

    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command.output().ok()?;

    if !output.status.success() {
        return None;
    }

    serde_json::from_slice(&output.stdout).ok()
}

fn powershell_single_quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

fn build_app_entry(
    name: String,
    path: String,
    launch_args: String,
    working_dir: String,
    source: String,
) -> AppEntry {
    let id = stable_id(&source, &path, &launch_args);
    AppEntry {
        id: id.clone(),
        initials: initials(&name),
        accent: accent_color(&id),
        name,
        path,
        launch_args,
        working_dir,
        launches: 0,
        source,
    }
}

fn stable_id(source: &str, path: &str, launch_args: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.to_lowercase().hash(&mut hasher);
    path.to_lowercase().hash(&mut hasher);
    launch_args.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("应用")
        .trim()
        .to_string()
}

fn parent_dir(path: &Path) -> String {
    path.parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn initials(name: &str) -> String {
    let chars: String = name
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .take(2)
        .collect();

    if chars.is_empty() {
        String::from("APP")
    } else {
        chars.to_uppercase()
    }
}

fn accent_color(id: &str) -> String {
    let palette = [
        "#2563eb", "#0f766e", "#7c3aed", "#c2410c", "#be123c", "#047857", "#4338ca", "#b45309",
        "#0369a1", "#6d28d9",
    ];
    let index = id
        .as_bytes()
        .iter()
        .fold(0usize, |acc, value| acc.wrapping_add(*value as usize))
        % palette.len();
    palette[index].to_string()
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

fn destroy_retained_windows(app: &AppHandle) {
    for label in [MAIN_LABEL, SETTINGS_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.destroy();
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        position_main_window(&window);
        let _ = window.set_skip_taskbar(true);
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
        .skip_taskbar(true)
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
    let x = work_area.position.x + work_area.size.width as i32
        - size.width as i32
        - settings.window_right_offset.max(0);
    let y = work_area.position.y + work_area.size.height as i32
        - size.height as i32
        - settings.window_bottom_offset.max(0);

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_settings_window(app: &AppHandle) {
    show_aux_window(
        app,
        SETTINGS_LABEL,
        "设置",
        "index.html?view=settings",
        500.0,
        320.0,
    );
}

fn show_about_window(app: &AppHandle) {
    show_aux_window(
        app,
        ABOUT_LABEL,
        "关于",
        "index.html?view=about",
        360.0,
        260.0,
    );
}

fn show_aux_window(app: &AppHandle, label: &str, title: &str, url: &str, width: f64, height: f64) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.center();
        let _ = window.set_skip_taskbar(true);
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    match WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(width, height)
        .resizable(false)
        .decorations(true)
        .skip_taskbar(true)
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
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            dismiss_webview_window(&close_window);
        }
        WindowEvent::Focused(true) => {
            was_focused.store(true, Ordering::Relaxed);
        }
        WindowEvent::Focused(false) => {
            let close_window = close_window.clone();
            let was_focused = was_focused.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(120));
                if was_focused.load(Ordering::Relaxed)
                    && !close_window.is_focused().unwrap_or(false)
                    && close_window.is_visible().unwrap_or(false)
                {
                    dismiss_webview_window(&close_window);
                }
            });
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
    let lightweight =
        CheckMenuItem::with_id(app, "lightweight", "轻量模式", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&about, &scan, &settings, &lightweight, &quit])?;
    let lightweight_for_event = lightweight.clone();

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("TauriLaunch")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "about" => show_about_window(app),
            "scan" => match scan_store_and_emit(app) {
                Ok(apps) => println!("scanned {} apps", apps.len()),
                Err(error) => eprintln!("scan failed: {error}"),
            },
            "settings" => show_settings_window(app),
            "lightweight" => {
                let checked = lightweight_for_event.is_checked().unwrap_or(false);
                LIGHTWEIGHT_MODE.store(checked, Ordering::Relaxed);
                if checked {
                    destroy_retained_windows(app);
                }
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
            let scan_app = app.handle().clone();
            thread::spawn(move || {
                if let Err(error) = scan_store_and_emit(&scan_app) {
                    eprintln!("startup scan failed: {error}");
                }
            });
            let settings = load_settings().unwrap_or_default();
            if should_show_main_window_on_launch(&settings) {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dismiss_main_window,
            dismiss_after_launch,
            get_apps,
            scan_apps,
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
