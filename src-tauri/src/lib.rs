use ib_pinyin::{matcher::PinyinMatcher, pinyin::PinyinNotation};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::os::windows::{ffi::OsStrExt, process::CommandExt};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, OnceLock,
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
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

const MAIN_LABEL: &str = "main";
const SETTINGS_LABEL: &str = "settings";
const ABOUT_LABEL: &str = "about";
const DEFAULT_RIGHT_OFFSET: i32 = 10;
const DEFAULT_BOTTOM_OFFSET: i32 = 10;
const DEFAULT_ICON_SIZE: u32 = 38;
const ICON_CACHE_SIZE: u32 = 128;
const LIGHTWEIGHT_DESTROY_DELAY: Duration = Duration::from_secs(60);
const AUTOSTART_NAME: &str = "TauriLaunch";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(target_os = "windows")]
const ERROR_ELEVATION_REQUIRED: i32 = 740;
static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);
static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);
static MAIN_DIALOG_OPEN: AtomicBool = AtomicBool::new(false);
static MAIN_DESTROY_GENERATION: AtomicU64 = AtomicU64::new(0);
static SETTINGS_DESTROY_GENERATION: AtomicU64 = AtomicU64::new(0);
static SCAN_MENU_ITEM: OnceLock<Mutex<Option<MenuItem<tauri::Wry>>>> = OnceLock::new();

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

fn default_icon_size() -> u32 {
    DEFAULT_ICON_SIZE
}

fn default_watched_directories() -> Vec<String> {
    Vec::new()
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
    #[serde(default = "default_icon_size")]
    icon_size: u32,
    #[serde(default)]
    autostart_enabled: bool,
    #[serde(default)]
    physical_delete_enabled: bool,
    #[serde(default)]
    show_hidden_apps: bool,
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
            icon_size: default_icon_size(),
            autostart_enabled: is_autostart_enabled(),
            physical_delete_enabled: false,
            show_hidden_apps: false,
            watched_directories: default_watched_directories(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppEntry {
    id: String,
    name: String,
    #[serde(default)]
    custom_name: String,
    path: String,
    launch_args: String,
    working_dir: String,
    launches: u32,
    accent: String,
    initials: String,
    #[serde(default)]
    search_text: String,
    #[serde(default)]
    icon_path: String,
    source: String,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    last_error: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShortcutInfo {
    target_path: String,
    arguments: String,
    working_directory: String,
    #[serde(default)]
    app_user_model_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StartAppInfo {
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

#[derive(Debug, Clone, Default)]
struct AppMetadata {
    launches: u32,
    custom_name: String,
    hidden: bool,
    last_error: String,
}

fn default_right_offset() -> i32 {
    DEFAULT_RIGHT_OFFSET
}

fn default_bottom_offset() -> i32 {
    DEFAULT_BOTTOM_OFFSET
}

#[tauri::command]
fn get_settings() -> AppSettings {
    let mut settings = load_settings().unwrap_or_default();
    settings.autostart_enabled = is_autostart_enabled();
    settings
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    configure_autostart(settings.autostart_enabled).map_err(|error| error.to_string())?;
    store_settings(&settings).map_err(|error| error.to_string())?;
    let _ = app.emit("settings-updated", settings);
    Ok(())
}

#[tauri::command]
fn get_apps() -> Vec<AppEntry> {
    configured_apps(load_apps().unwrap_or_default())
}

#[tauri::command]
fn scan_apps(app: AppHandle) {
    spawn_scan(app);
}

#[tauri::command]
fn set_main_dialog_open(open: bool) {
    MAIN_DIALOG_OPEN.store(open, Ordering::Relaxed);
}

#[tauri::command]
fn search_apps(query: String) -> Vec<AppEntry> {
    filter_apps(configured_apps(load_apps().unwrap_or_default()), &query)
}

#[tauri::command]
fn add_app(path: String) -> Result<Vec<AppEntry>, String> {
    add_app_from_source(Path::new(&path)).map_err(|error| error.to_string())
}

#[tauri::command]
fn launch_app(app: AppHandle, app_id: String) -> Result<Vec<AppEntry>, String> {
    let apps = launch_stored_app(&app_id).map_err(|error| error.to_string())?;
    let _ = app.emit("apps-updated", apps.clone());
    dismiss_window(&app, MAIN_LABEL);
    Ok(apps)
}

#[tauri::command]
fn hide_app(app: AppHandle, app_id: String) -> Result<Vec<AppEntry>, String> {
    let apps = hide_stored_app(&app_id).map_err(|error| error.to_string())?;
    let _ = app.emit("apps-updated", apps.clone());
    Ok(apps)
}

#[tauri::command]
fn pin_app(app: AppHandle, app_id: String) -> Result<Vec<AppEntry>, String> {
    let apps = pin_stored_app(&app_id).map_err(|error| error.to_string())?;
    let _ = app.emit("apps-updated", apps.clone());
    Ok(apps)
}

#[tauri::command]
fn reset_app_position(app: AppHandle, app_id: String) -> Result<Vec<AppEntry>, String> {
    let apps = reset_stored_app_position(&app_id).map_err(|error| error.to_string())?;
    let _ = app.emit("apps-updated", apps.clone());
    Ok(apps)
}

#[tauri::command]
fn rename_app(app: AppHandle, app_id: String, name: String) -> Result<Vec<AppEntry>, String> {
    let apps = rename_stored_app(&app_id, &name).map_err(|error| error.to_string())?;
    let _ = app.emit("apps-updated", apps.clone());
    Ok(apps)
}

#[tauri::command]
fn open_app_directory(app_id: String) -> Result<(), String> {
    open_stored_app_directory(&app_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn dismiss_main_window(app: AppHandle) {
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

fn icons_dir() -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join("icons"))
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

fn spawn_scan(app: AppHandle) {
    if SCAN_RUNNING.swap(true, Ordering::Relaxed) {
        emit_scan_state(&app, true);
        return;
    }

    set_scan_active(&app, true);
    thread::spawn(move || {
        if let Err(error) = scan_store_and_emit(&app) {
            eprintln!("scan failed: {error}");
            emit_scan_failed_to_visible_main(&app, &error.to_string());
        }
        SCAN_RUNNING.store(false, Ordering::Relaxed);
        set_scan_active(&app, false);
    });
}

fn set_scan_active(app: &AppHandle, active: bool) {
    if let Some(item) = SCAN_MENU_ITEM
        .get()
        .and_then(|item| item.lock().ok().and_then(|item| item.clone()))
    {
        let _ = item.set_enabled(!active);
    }

    emit_scan_state(app, active);
}

fn emit_scan_state(app: &AppHandle, active: bool) {
    let _ = app.emit("scan-state-changed", active);
}

fn scan_store_and_emit(app: &AppHandle) -> io::Result<Vec<AppEntry>> {
    let apps = scan_configured_apps()?;
    store_apps(&apps)?;
    let visible = configured_apps(apps);
    emit_apps_updated_to_visible_main(app, &visible);
    Ok(visible)
}

fn emit_apps_updated_to_visible_main(app: &AppHandle, apps: &[AppEntry]) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };

    if !window.is_visible().unwrap_or(false) {
        return;
    }

    let _ = window.emit("apps-updated", apps.to_vec());
}

fn emit_scan_failed_to_visible_main(app: &AppHandle, error: &str) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };

    if !window.is_visible().unwrap_or(false) {
        return;
    }

    let _ = window.emit("scan-failed", error.to_string());
}

fn filter_apps(apps: Vec<AppEntry>, query: &str) -> Vec<AppEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return apps;
    }

    let matcher = PinyinMatcher::builder(query.as_str())
        .pinyin_notations(PinyinNotation::Ascii | PinyinNotation::AsciiFirstLetter)
        .is_pattern_partial(true)
        .build();

    apps.into_iter()
        .filter(|app| {
            app.search_text.to_lowercase().contains(&query) || matcher.is_match(app.name.as_str())
        })
        .collect()
}

fn configured_apps(apps: Vec<AppEntry>) -> Vec<AppEntry> {
    let settings = load_settings().unwrap_or_default();
    apps_for_settings(apps, &settings)
}

fn apps_for_settings(mut apps: Vec<AppEntry>, settings: &AppSettings) -> Vec<AppEntry> {
    sort_apps(&mut apps);
    if settings.show_hidden_apps {
        apps
    } else {
        apps.into_iter().filter(|app| !app.hidden).collect()
    }
}

fn sort_apps(apps: &mut [AppEntry]) {
    apps.sort_by(|left, right| {
        left.hidden
            .cmp(&right.hidden)
            .then_with(|| right.launches.cmp(&left.launches))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
    });
}

fn app_accessible(app: &AppEntry, settings: &AppSettings) -> bool {
    !app.hidden || settings.show_hidden_apps
}

fn remove_icon_cache(app: &AppEntry) {
    let icon_path = app.icon_path.trim();
    if !icon_path.is_empty() {
        let _ = fs::remove_file(icon_path);
    }

    if let Ok(path) = expected_icon_cache_path_for(&app.id, &app.path) {
        let _ = fs::remove_file(path);
    }
}

fn add_app_from_source(source: &Path) -> io::Result<Vec<AppEntry>> {
    let mut app_entry = app_entry_from_source(source)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "请选择有效的 .exe 或 .lnk 文件",
        )
    })?;

    if app_entry.path.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "应用启动路径为空",
        ));
    }

    let mut apps = load_apps()?;
    if let Some(existing) = apps.iter_mut().find(|app| app.id == app_entry.id) {
        if existing.hidden {
            app_entry.custom_name = existing.custom_name.clone();
            if !app_entry.custom_name.trim().is_empty() {
                apply_display_name(&mut app_entry, existing.custom_name.clone());
            }
            app_entry.hidden = false;
            app_entry.launches = 0;
            *existing = app_entry;
        }
        sort_apps(&mut apps);
        store_apps(&apps)?;
        return Ok(configured_apps(apps));
    }

    if let Some(existing) = apps
        .iter_mut()
        .find(|app| normalize_key(&app.source) == normalize_key(&app_entry.source))
    {
        if existing.hidden {
            app_entry.custom_name = existing.custom_name.clone();
            if !app_entry.custom_name.trim().is_empty() {
                apply_display_name(&mut app_entry, existing.custom_name.clone());
            }
            app_entry.hidden = false;
            app_entry.launches = 0;
            *existing = app_entry;
        }
        sort_apps(&mut apps);
        store_apps(&apps)?;
        return Ok(configured_apps(apps));
    }

    apps.push(app_entry);
    sort_apps(&mut apps);
    store_apps(&apps)?;
    Ok(configured_apps(apps))
}

fn launch_stored_app(app_id: &str) -> io::Result<Vec<AppEntry>> {
    let mut apps = load_apps()?;
    let settings = load_settings().unwrap_or_default();
    let Some(index) = apps
        .iter()
        .position(|app| app.id == app_id && app_accessible(app, &settings))
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "应用不存在或已隐藏",
        ));
    };

    let launch_result = start_process(&apps[index]);
    match launch_result {
        Ok(()) => {
            apps[index].launches = apps[index].launches.saturating_add(1);
            apps[index].last_error.clear();
            sort_apps(&mut apps);
            store_apps(&apps)?;
            Ok(configured_apps(apps))
        }
        Err(error) => {
            apps[index].last_error = error.to_string();
            store_apps(&apps)?;
            Err(error)
        }
    }
}

fn hide_stored_app(app_id: &str) -> io::Result<Vec<AppEntry>> {
    let mut apps = load_apps()?;
    let settings = load_settings().unwrap_or_default();
    if settings.physical_delete_enabled {
        let Some(index) = apps.iter().position(|app| app.id == app_id) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "app not found"));
        };

        let app = apps.remove(index);
        remove_icon_cache(&app);
        sort_apps(&mut apps);
        store_apps(&apps)?;
        return Ok(configured_apps(apps));
    }

    let Some(app) = apps.iter_mut().find(|app| app.id == app_id) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "应用不存在"));
    };

    app.hidden = true;
    sort_apps(&mut apps);
    store_apps(&apps)?;
    Ok(configured_apps(apps))
}

fn pin_stored_app(app_id: &str) -> io::Result<Vec<AppEntry>> {
    let mut apps = load_apps()?;
    let settings = load_settings().unwrap_or_default();
    let max_launches = apps
        .iter()
        .filter(|app| app_accessible(app, &settings))
        .map(|app| app.launches)
        .max()
        .unwrap_or_default();
    let Some(app) = apps
        .iter_mut()
        .find(|app| app.id == app_id && app_accessible(app, &settings))
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "应用不存在或已隐藏",
        ));
    };

    app.launches = max_launches.saturating_add(2);
    sort_apps(&mut apps);
    store_apps(&apps)?;
    Ok(configured_apps(apps))
}

fn reset_stored_app_position(app_id: &str) -> io::Result<Vec<AppEntry>> {
    let mut apps = load_apps()?;
    let settings = load_settings().unwrap_or_default();
    let Some(app) = apps
        .iter_mut()
        .find(|app| app.id == app_id && app_accessible(app, &settings))
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "应用不存在或已隐藏",
        ));
    };

    app.launches = 0;
    sort_apps(&mut apps);
    store_apps(&apps)?;
    Ok(configured_apps(apps))
}

fn rename_stored_app(app_id: &str, name: &str) -> io::Result<Vec<AppEntry>> {
    let mut apps = load_apps()?;
    let settings = load_settings().unwrap_or_default();
    let name = name.trim();
    if name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "名称不能为空"));
    }

    let Some(app) = apps
        .iter_mut()
        .find(|app| app.id == app_id && app_accessible(app, &settings))
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "应用不存在或已隐藏",
        ));
    };

    app.custom_name = name.to_string();
    apply_display_name(app, name.to_string());
    sort_apps(&mut apps);
    store_apps(&apps)?;
    Ok(configured_apps(apps))
}

fn open_stored_app_directory(app_id: &str) -> io::Result<()> {
    let apps = load_apps()?;
    let settings = load_settings().unwrap_or_default();
    let Some(app) = apps
        .iter()
        .find(|app| app.id == app_id && app_accessible(app, &settings))
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "应用不存在或已隐藏",
        ));
    };
    let Some(directory) = Path::new(&app.path).parent() else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "所在目录不存在"));
    };

    let mut command = Command::new("explorer.exe");
    command.arg(directory);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command.spawn().map(|_| ())
}

fn start_process(app: &AppEntry) -> io::Result<()> {
    if is_shell_apps_folder_path(&app.path) {
        #[cfg(target_os = "windows")]
        return shell_execute(app, "open", None);

        #[cfg(not(target_os = "windows"))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "shell AppsFolder launch is only supported on Windows",
        ));
    }

    let path = PathBuf::from(&app.path);
    let args = split_launch_args(&app.launch_args)?;
    let mut command = Command::new(&path);
    command.args(args);

    let working_dir = if app.working_dir.trim().is_empty() {
        path.parent().map(Path::to_path_buf)
    } else {
        Some(PathBuf::from(app.working_dir.trim()))
    };

    if let Some(working_dir) = &working_dir {
        command.current_dir(working_dir);
    }

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    match command.spawn() {
        Ok(_) => Ok(()),
        Err(error) => {
            #[cfg(target_os = "windows")]
            {
                if error.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) {
                    return shell_execute(app, "runas", working_dir.as_deref());
                }
            }

            Err(error)
        }
    }
}

#[cfg(target_os = "windows")]
fn shell_execute(app: &AppEntry, operation: &str, working_dir: Option<&Path>) -> io::Result<()> {
    let operation = wide_null(operation);
    let file = wide_null(app.path.trim());
    let parameters = wide_null(app.launch_args.trim());
    let directory_text = working_dir
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let directory = wide_null(directory_text.as_str());
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            if directory_text.is_empty() {
                std::ptr::null()
            } else {
                directory.as_ptr()
            },
            SW_SHOWNORMAL,
        )
    };

    if result as isize > 32 {
        return Ok(());
    }

    Err(io::Error::from_raw_os_error(result as i32))
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn split_launch_args(value: &str) -> io::Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaping = false;

    for ch in value.chars() {
        if escaping {
            if ch != '"' && ch != '\\' {
                current.push('\\');
            }
            current.push(ch);
            escaping = false;
            continue;
        }

        match ch {
            '\\' if in_quotes => escaping = true,
            '"' => in_quotes = !in_quotes,
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if escaping {
        current.push('\\');
    }

    if in_quotes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "启动参数引号不完整",
        ));
    }

    if !current.is_empty() {
        args.push(current);
    }

    Ok(args)
}

fn scan_configured_apps() -> io::Result<Vec<AppEntry>> {
    let settings = load_settings()?;
    let previous_apps = load_apps()?;
    let previous_metadata: HashMap<String, AppMetadata> = previous_apps
        .iter()
        .map(|app| (app.id.clone(), metadata_from_app(app)))
        .collect();
    let previous_source_metadata: HashMap<String, AppMetadata> = previous_apps
        .iter()
        .map(|app| (normalize_key(&app.source), metadata_from_app(app)))
        .collect();
    let previous_launch_metadata: HashMap<String, AppMetadata> = previous_apps
        .iter()
        .map(|app| (launch_metadata_key(app), metadata_from_app(app)))
        .collect();
    let previous_by_source: HashMap<String, AppEntry> = previous_apps
        .into_iter()
        .map(|app| (normalize_key(&app.source), app))
        .collect();
    let mut apps = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for directory in settings.watched_directories {
        let root = PathBuf::from(directory);
        if !root.is_dir() {
            continue;
        }
        scan_directory(
            &root,
            &previous_metadata,
            &previous_source_metadata,
            &previous_launch_metadata,
            &previous_by_source,
            &mut seen,
            &mut apps,
        )?;
    }
    scan_start_apps(
        &previous_metadata,
        &previous_source_metadata,
        &previous_launch_metadata,
        &previous_by_source,
        &mut seen,
        &mut apps,
    );

    sort_apps(&mut apps);
    Ok(apps)
}

fn scan_directory(
    root: &Path,
    previous_metadata: &HashMap<String, AppMetadata>,
    previous_source_metadata: &HashMap<String, AppMetadata>,
    previous_launch_metadata: &HashMap<String, AppMetadata>,
    previous_by_source: &HashMap<String, AppEntry>,
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

            let source_key = normalize_key(path.to_string_lossy().as_ref());
            let previous_app = previous_by_source.get(&source_key);
            let refresh_icon = previous_app.is_some();
            let app_entry = match extension.as_str() {
                "exe" | "lnk" => {
                    if let Some(previous_app) =
                        previous_app.filter(|app| can_reuse_scanned_app(app))
                    {
                        if push_scanned_app(previous_app.clone(), seen, apps) {
                            continue;
                        }
                    }

                    match extension.as_str() {
                        "exe" => Some(app_entry_from_exe(&path, refresh_icon)),
                        "lnk" => app_entry_from_shortcut(&path, refresh_icon),
                        _ => None,
                    }
                }
                _ => None,
            };

            if let Some(mut app_entry) = app_entry {
                if app_entry.path.is_empty() {
                    continue;
                }

                if let Some(metadata) = find_previous_metadata(
                    &app_entry,
                    previous_metadata,
                    previous_source_metadata,
                    previous_launch_metadata,
                ) {
                    apply_metadata(&mut app_entry, metadata);
                }
                push_scanned_app(app_entry, seen, apps);
            }
        }
    }

    Ok(())
}

fn scan_start_apps(
    previous_metadata: &HashMap<String, AppMetadata>,
    previous_source_metadata: &HashMap<String, AppMetadata>,
    previous_launch_metadata: &HashMap<String, AppMetadata>,
    previous_by_source: &HashMap<String, AppEntry>,
    seen: &mut HashMap<String, ()>,
    apps: &mut Vec<AppEntry>,
) {
    for start_app in read_start_apps() {
        let app_user_model_id = start_app.app_id.trim();
        let name = start_app.name.trim();
        if app_user_model_id.is_empty() || name.is_empty() {
            continue;
        }

        let source = shell_apps_folder_path(app_user_model_id);
        if let Some(previous_app) = previous_by_source
            .get(&normalize_key(&source))
            .filter(|app| can_reuse_scanned_app(app))
        {
            if push_scanned_app(previous_app.clone(), seen, apps) {
                continue;
            }
        }

        let mut app_entry = build_app_entry(
            name.to_string(),
            source.clone(),
            String::new(),
            String::new(),
            source,
            false,
        );

        if let Some(metadata) = find_previous_metadata(
            &app_entry,
            previous_metadata,
            previous_source_metadata,
            previous_launch_metadata,
        ) {
            apply_metadata(&mut app_entry, metadata);
        }
        push_scanned_app(app_entry, seen, apps);
    }
}

fn read_start_apps() -> Vec<StartAppInfo> {
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::UTF8
Get-StartApps |
  Where-Object { $_.AppID -like '*!*' } |
  Select-Object Name, AppID |
  ConvertTo-Json -Compress
"#;

    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let Ok(output) = command.output() else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    parse_start_apps_json(&output.stdout)
}

fn parse_start_apps_json(value: &[u8]) -> Vec<StartAppInfo> {
    if value.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Vec::new();
    }

    serde_json::from_slice::<Vec<StartAppInfo>>(value)
        .or_else(|_| serde_json::from_slice::<StartAppInfo>(value).map(|app| vec![app]))
        .unwrap_or_default()
}

fn app_entry_from_source(path: &Path) -> io::Result<Option<AppEntry>> {
    if !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "文件不存在"));
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();

    Ok(match extension.as_str() {
        "exe" => Some(app_entry_from_exe(path, false)),
        "lnk" => app_entry_from_shortcut(path, false),
        _ => None,
    })
}

fn metadata_from_app(app: &AppEntry) -> AppMetadata {
    AppMetadata {
        launches: app.launches,
        custom_name: app.custom_name.clone(),
        hidden: app.hidden,
        last_error: app.last_error.clone(),
    }
}

fn find_previous_metadata<'a>(
    app: &AppEntry,
    previous_metadata: &'a HashMap<String, AppMetadata>,
    previous_source_metadata: &'a HashMap<String, AppMetadata>,
    previous_launch_metadata: &'a HashMap<String, AppMetadata>,
) -> Option<&'a AppMetadata> {
    previous_metadata
        .get(&app.id)
        .or_else(|| previous_source_metadata.get(&normalize_key(&app.source)))
        .or_else(|| previous_launch_metadata.get(&launch_metadata_key(app)))
}

fn apply_metadata(app: &mut AppEntry, metadata: &AppMetadata) {
    app.launches = metadata.launches;
    app.hidden = metadata.hidden;
    app.last_error = metadata.last_error.clone();
    app.custom_name = metadata.custom_name.clone();
    if !metadata.custom_name.trim().is_empty() {
        apply_display_name(app, metadata.custom_name.clone());
    }
}

fn launch_metadata_key(app: &AppEntry) -> String {
    format!("{}\n{}", normalize_key(&app.path), app.launch_args)
}

fn push_scanned_app(
    app: AppEntry,
    seen: &mut HashMap<String, ()>,
    apps: &mut Vec<AppEntry>,
) -> bool {
    if seen.insert(scan_dedupe_key(&app), ()).is_some() {
        return false;
    }

    apps.push(app);
    true
}

fn scan_dedupe_key(app: &AppEntry) -> String {
    format!(
        "{}\n{}\n{}",
        normalize_key(&app.path),
        app.launch_args,
        normalize_key(&app.source)
    )
}

fn normalize_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn can_reuse_scanned_app(app: &AppEntry) -> bool {
    app_launch_target_exists(app)
        && app_icon_cache_current(app).unwrap_or(false)
        && split_launch_args(&app.launch_args).is_ok()
}

fn app_launch_target_exists(app: &AppEntry) -> bool {
    if is_shell_apps_folder_path(&app.path) {
        return true;
    }

    let path = Path::new(app.path.trim());
    if app.path.trim().is_empty() || !path.is_file() {
        return false;
    }

    let working_dir = app.working_dir.trim();
    if working_dir.is_empty() {
        return true;
    }

    Path::new(working_dir).is_dir()
}

fn is_shell_apps_folder_path(value: &str) -> bool {
    value
        .trim()
        .to_ascii_lowercase()
        .starts_with("shell:appsfolder\\")
}

fn app_icon_cache_current(app: &AppEntry) -> io::Result<bool> {
    let icon_path = app.icon_path.trim();
    if icon_path.is_empty() {
        return Ok(false);
    }

    let icon_path = Path::new(icon_path);
    let expected_icon_path = expected_icon_cache_path_for(&app.id, &app.path)?;
    Ok(normalize_key(icon_path.to_string_lossy().as_ref())
        == normalize_key(expected_icon_path.to_string_lossy().as_ref())
        && icon_path.is_file())
}

fn app_entry_from_exe(path: &Path, refresh_icon: bool) -> AppEntry {
    let path_text = path.to_string_lossy().into_owned();
    let name = file_stem(path);
    build_app_entry(
        name,
        path_text.clone(),
        String::new(),
        parent_dir(path),
        path_text,
        refresh_icon,
    )
}

fn app_entry_from_shortcut(path: &Path, refresh_icon: bool) -> Option<AppEntry> {
    let shortcut = read_shortcut(path)?;
    let mut target_path = shortcut.target_path.trim().to_string();
    let app_user_model_id = shortcut.app_user_model_id.trim();
    if !target_path.is_empty()
        && !Path::new(&target_path).is_file()
        && !app_user_model_id.is_empty()
    {
        target_path = shell_apps_folder_path(app_user_model_id);
    }

    if target_path.is_empty() && !app_user_model_id.is_empty() {
        target_path = shell_apps_folder_path(app_user_model_id);
    }

    if target_path.is_empty()
        || (!is_shell_apps_folder_path(&target_path) && !Path::new(&target_path).is_file())
    {
        return None;
    }

    let name = file_stem(path);
    let is_shell_app = is_shell_apps_folder_path(&target_path);
    let source = if is_shell_app {
        target_path.clone()
    } else {
        path.to_string_lossy().into_owned()
    };
    Some(build_app_entry(
        name,
        target_path,
        shortcut.arguments.trim().to_string(),
        if is_shell_app {
            String::new()
        } else {
            shortcut.working_directory.trim().to_string()
        },
        source,
        refresh_icon,
    ))
}

fn shell_apps_folder_path(app_user_model_id: &str) -> String {
    format!("shell:AppsFolder\\{}", app_user_model_id.trim())
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
  AppUserModelID = $shortcutPath | ForEach-Object {{
    $folderPath = Split-Path -LiteralPath $_
    $fileName = Split-Path -Leaf $_
    $folder = (New-Object -ComObject Shell.Application).Namespace($folderPath)
    if ($null -eq $folder) {{ return '' }}
    $item = $folder.ParseName($fileName)
    if ($null -eq $item) {{ return '' }}
    $item.ExtendedProperty('System.AppUserModel.ID')
  }}
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

fn powershell_single_quoted_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn build_app_entry(
    name: String,
    path: String,
    launch_args: String,
    working_dir: String,
    source: String,
    refresh_icon: bool,
) -> AppEntry {
    let id = stable_id(&source, &path, &launch_args);
    let initials = initials(&name);
    let search_text = search_text(&name, &path, &launch_args, &working_dir, &source, &initials);
    AppEntry {
        id: id.clone(),
        custom_name: String::new(),
        initials,
        search_text,
        icon_path: ensure_icon_cache(&id, &source, &path, refresh_icon).unwrap_or_default(),
        accent: accent_color(&id),
        name,
        path,
        launch_args,
        working_dir,
        launches: 0,
        source,
        hidden: false,
        last_error: String::new(),
    }
}

fn apply_display_name(app: &mut AppEntry, name: String) {
    app.name = name;
    app.initials = initials(&app.name);
    app.search_text = search_text(
        &app.name,
        &app.path,
        &app.launch_args,
        &app.working_dir,
        &app.source,
        &app.initials,
    );
}

fn expected_icon_cache_path(id: &str) -> io::Result<PathBuf> {
    Ok(icons_dir()?.join(format!("{id}-{ICON_CACHE_SIZE}.png")))
}

fn expected_icon_cache_path_for(id: &str, path: &str) -> io::Result<PathBuf> {
    if is_shell_apps_folder_path(path) {
        return Ok(icons_dir()?.join(format!("{id}-shell-{ICON_CACHE_SIZE}.png")));
    }

    expected_icon_cache_path(id)
}

fn ensure_icon_cache(id: &str, source: &str, path: &str, refresh: bool) -> io::Result<String> {
    let icon_path = expected_icon_cache_path_for(id, path)?;
    if let Some(directory) = icon_path.parent() {
        fs::create_dir_all(directory)?;
    }

    if icon_path.exists() && !refresh {
        return Ok(icon_path.to_string_lossy().into_owned());
    }

    if refresh && icon_path.exists() {
        let _ = fs::remove_file(&icon_path);
    }

    extract_icon_to_png(source, path, &icon_path)?;
    Ok(icon_path.to_string_lossy().into_owned())
}

fn extract_icon_to_png(source: &str, path: &str, output: &Path) -> io::Result<()> {
    if is_shell_apps_folder_path(path) {
        return extract_shell_app_icon_to_png(path, output);
    }

    let source = powershell_single_quoted_text(source);
    let path = powershell_single_quoted_text(path);
    let output = powershell_single_quoted(output);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class IconNative {{
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern uint PrivateExtractIcons(string lpszFile, int nIconIndex, int cxIcon, int cyIcon, IntPtr[] phicon, uint[] piconid, uint nIcons, uint flags);

  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool DestroyIcon(IntPtr hIcon);
}}
"@
$outputPath = {output}
$candidates = @({path}, {source})
$sizes = @({ICON_CACHE_SIZE}, 64, 48, 32)
foreach ($candidate in $candidates) {{
  if ([string]::IsNullOrWhiteSpace($candidate) -or -not (Test-Path -LiteralPath $candidate)) {{
    continue
  }}
  foreach ($size in $sizes) {{
    $handles = New-Object IntPtr[] 1
    $ids = New-Object UInt32[] 1
    $count = [IconNative]::PrivateExtractIcons($candidate, 0, $size, $size, $handles, $ids, 1, 0)
    if ($count -gt 0 -and $handles[0] -ne [IntPtr]::Zero) {{
      try {{
        $sourceIcon = [System.Drawing.Icon]::FromHandle($handles[0])
        $icon = [System.Drawing.Icon]$sourceIcon.Clone()
        $bitmap = $icon.ToBitmap()
        $bitmap.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
        $icon.Dispose()
        exit 0
      }} finally {{
        [IconNative]::DestroyIcon($handles[0]) | Out-Null
      }}
    }}
  }}
}}

foreach ($candidate in $candidates) {{
  if ([string]::IsNullOrWhiteSpace($candidate) -or -not (Test-Path -LiteralPath $candidate)) {{
    continue
  }}
  try {{
    $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($candidate)
    if ($null -ne $icon) {{
      $bitmap = $icon.ToBitmap()
      $bitmap.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
      $bitmap.Dispose()
      $icon.Dispose()
      exit 0
    }}
  }} catch {{}}
}}
exit 1
"#
    );

    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn extract_shell_app_icon_to_png(path: &str, output: &Path) -> io::Result<()> {
    let app_id = path
        .trim()
        .strip_prefix("shell:AppsFolder\\")
        .or_else(|| path.trim().strip_prefix("shell:appsfolder\\"))
        .unwrap_or_default();
    if app_id.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing app user model id",
        ));
    }

    let app_id = powershell_single_quoted_text(app_id);
    let output = powershell_single_quoted(output);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$appUserModelId = {app_id}
$outputPath = {output}
$parts = $appUserModelId.Split('!', 2)
if ($parts.Length -ne 2 -or [string]::IsNullOrWhiteSpace($parts[0]) -or [string]::IsNullOrWhiteSpace($parts[1])) {{
  exit 1
}}

$packageFamilyName = $parts[0]
$applicationId = $parts[1]
$package = Get-AppxPackage | Where-Object {{ $_.PackageFamilyName -eq $packageFamilyName }} | Select-Object -First 1
if ($null -eq $package -or [string]::IsNullOrWhiteSpace($package.InstallLocation)) {{
  exit 1
}}

$manifestPath = Join-Path $package.InstallLocation 'AppxManifest.xml'
if (-not (Test-Path -LiteralPath $manifestPath)) {{
  exit 1
}}

[xml]$manifest = Get-Content -LiteralPath $manifestPath -Raw
$application = $manifest.SelectNodes("//*[local-name()='Application']") |
  Where-Object {{ $_.GetAttribute('Id') -eq $applicationId }} |
  Select-Object -First 1
if ($null -eq $application) {{
  exit 1
}}

$visual = $application.SelectSingleNode("*[local-name()='VisualElements']")
$logoValues = New-Object System.Collections.Generic.List[string]
if ($null -ne $visual) {{
  foreach ($name in @('Square150x150Logo', 'Square44x44Logo')) {{
    $value = $visual.GetAttribute($name)
    if (-not [string]::IsNullOrWhiteSpace($value)) {{
      $logoValues.Add($value)
    }}
  }}
}}

$packageLogo = $manifest.SelectSingleNode("//*[local-name()='Properties']/*[local-name()='Logo']")
if ($null -ne $packageLogo -and -not [string]::IsNullOrWhiteSpace($packageLogo.InnerText)) {{
  $logoValues.Add($packageLogo.InnerText)
}}

function Add-LogoCandidate {{
  param([System.Collections.Generic.List[string]]$Candidates, [string]$BasePath)
  if ([string]::IsNullOrWhiteSpace($BasePath)) {{
    return
  }}

  $fullPath = Join-Path $package.InstallLocation $BasePath
  if (Test-Path -LiteralPath $fullPath) {{
    $Candidates.Add($fullPath)
  }}

  $directory = Split-Path -Parent $fullPath
  $leaf = Split-Path -Leaf $fullPath
  $stem = [IO.Path]::GetFileNameWithoutExtension($leaf)
  if ((Test-Path -LiteralPath $directory) -and -not [string]::IsNullOrWhiteSpace($stem)) {{
    Get-ChildItem -LiteralPath $directory -File -Filter "$stem*.png" -ErrorAction SilentlyContinue |
      Sort-Object Length -Descending |
      ForEach-Object {{ $Candidates.Add($_.FullName) }}
  }}
}}

$candidates = New-Object System.Collections.Generic.List[string]
foreach ($logoValue in $logoValues) {{
  Add-LogoCandidate $candidates $logoValue
}}

foreach ($candidateInfo in $candidates |
  Select-Object -Unique |
  ForEach-Object {{ Get-Item -LiteralPath $_ -ErrorAction SilentlyContinue }} |
  Sort-Object Length -Descending) {{
  try {{
    $candidate = $candidateInfo.FullName
    if (-not (Test-Path -LiteralPath $candidate)) {{
      continue
    }}

    $sourceBitmap = [System.Drawing.Bitmap]::FromFile($candidate)
    try {{
      $bitmap = New-Object System.Drawing.Bitmap {ICON_CACHE_SIZE}, {ICON_CACHE_SIZE}
      $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
      try {{
        $graphics.Clear([System.Drawing.Color]::Transparent)
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $left = $sourceBitmap.Width
        $top = $sourceBitmap.Height
        $right = -1
        $bottom = -1
        for ($y = 0; $y -lt $sourceBitmap.Height; $y++) {{
          for ($x = 0; $x -lt $sourceBitmap.Width; $x++) {{
            if ($sourceBitmap.GetPixel($x, $y).A -gt 8) {{
              if ($x -lt $left) {{ $left = $x }}
              if ($x -gt $right) {{ $right = $x }}
              if ($y -lt $top) {{ $top = $y }}
              if ($y -gt $bottom) {{ $bottom = $y }}
            }}
          }}
        }}

        if ($right -lt $left -or $bottom -lt $top) {{
          continue
        }}

        $cropWidth = $right - $left + 1
        $cropHeight = $bottom - $top + 1
        $scale = [Math]::Min({ICON_CACHE_SIZE} / $cropWidth, {ICON_CACHE_SIZE} / $cropHeight)
        $drawWidth = [int][Math]::Max(1, [Math]::Round($cropWidth * $scale))
        $drawHeight = [int][Math]::Max(1, [Math]::Round($cropHeight * $scale))
        $x = [int](({ICON_CACHE_SIZE} - $drawWidth) / 2)
        $y = [int](({ICON_CACHE_SIZE} - $drawHeight) / 2)
        $destination = New-Object System.Drawing.Rectangle $x, $y, $drawWidth, $drawHeight
        $source = New-Object System.Drawing.Rectangle $left, $top, $cropWidth, $cropHeight
        $graphics.DrawImage($sourceBitmap, $destination, $source, [System.Drawing.GraphicsUnit]::Pixel)
        $bitmap.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
        exit 0
      }} finally {{
        $graphics.Dispose()
        $bitmap.Dispose()
      }}
    }} finally {{
      $sourceBitmap.Dispose()
    }}
  }} catch {{}}
}}

exit 1
"#
    );

    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn configure_autostart(enabled: bool) -> io::Result<()> {
    if enabled {
        enable_autostart()
    } else {
        disable_autostart()
    }
}

fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                AUTOSTART_NAME,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        return output
            .map(|output| output.status.success())
            .unwrap_or(false);
    }

    #[cfg(not(target_os = "windows"))]
    false
}

fn enable_autostart() -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe()?;
        let command_value = format!("\"{}\" --startup", exe.to_string_lossy());
        let status = Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                AUTOSTART_NAME,
                "/t",
                "REG_SZ",
                "/d",
                &command_value,
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()?;

        if status.success() {
            return Ok(());
        }

        return Err(io::Error::new(io::ErrorKind::Other, "写入开机启动失败"));
    }

    #[cfg(not(target_os = "windows"))]
    Ok(())
}

fn disable_autostart() -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                AUTOSTART_NAME,
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()?;

        if status.success() || !is_autostart_enabled() {
            return Ok(());
        }

        return Err(io::Error::new(io::ErrorKind::Other, "移除开机启动失败"));
    }

    #[cfg(not(target_os = "windows"))]
    Ok(())
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
    let english_words = split_english_words(name);
    if english_words.len() >= 2 {
        return english_words
            .iter()
            .take(4)
            .filter_map(|word| word.chars().next())
            .collect::<String>()
            .to_uppercase();
    }

    let chinese_initials = chinese_initials(name);
    if !chinese_initials.is_empty() {
        return chinese_initials.chars().take(5).collect::<String>();
    }

    if let Some(word) = english_words.first() {
        return word.chars().take(3).collect::<String>().to_uppercase();
    }

    String::from("APP")
}

fn search_text(
    name: &str,
    path: &str,
    launch_args: &str,
    working_dir: &str,
    source: &str,
    initials: &str,
) -> String {
    let english_words = split_english_words(name);
    let english_initials = english_words
        .iter()
        .filter_map(|word| word.chars().next())
        .collect::<String>();
    let mut tokens = vec![
        name.to_string(),
        path.to_string(),
        launch_args.to_string(),
        working_dir.to_string(),
        source.to_string(),
        initials.to_string(),
        english_initials,
        english_words.join(""),
    ];
    tokens.extend(english_words);

    tokens
        .into_iter()
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_english_words(value: &str) -> Vec<String> {
    let mut normalized = String::new();
    let mut previous_was_lower_or_digit = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if previous_was_lower_or_digit && ch.is_ascii_uppercase() {
                normalized.push(' ');
            }
            normalized.push(ch);
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            normalized.push(' ');
            previous_was_lower_or_digit = false;
        }
    }

    normalized
        .split_whitespace()
        .map(|word| word.to_string())
        .collect()
}

fn chinese_initials(value: &str) -> String {
    value
        .chars()
        .filter(|ch| is_cjk(*ch))
        .filter_map(|ch| {
            let text = ch.to_string();
            ('A'..='Z').find(|letter| {
                let pattern = letter.to_ascii_lowercase().to_string();
                PinyinMatcher::builder(pattern.as_str())
                    .pinyin_notations(PinyinNotation::AsciiFirstLetter)
                    .build()
                    .is_match(text.as_str())
            })
        })
        .collect()
}

fn is_cjk(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(name: &str) -> AppEntry {
        build_app_entry(
            name.to_string(),
            format!(r"C:\Apps\{name}\{name}.exe"),
            String::new(),
            format!(r"C:\Apps\{name}"),
            format!(r"C:\Apps\{name}\{name}.lnk"),
            false,
        )
    }

    fn matched_names(apps: Vec<AppEntry>, query: &str) -> Vec<String> {
        filter_apps(apps, query)
            .into_iter()
            .map(|app| app.name)
            .collect()
    }

    fn sorted_names(mut apps: Vec<AppEntry>) -> Vec<String> {
        sort_apps(&mut apps);
        apps.into_iter().map(|app| app.name).collect()
    }

    #[test]
    fn split_launch_args_handles_quotes() {
        assert_eq!(
            split_launch_args(r#"--profile "C:\Users\fengqi\App Data" --flag"#).unwrap(),
            vec![
                "--profile".to_string(),
                r"C:\Users\fengqi\App Data".to_string(),
                "--flag".to_string()
            ]
        );
    }

    #[test]
    fn split_launch_args_rejects_unclosed_quote() {
        assert!(split_launch_args(r#"--profile "broken"#).is_err());
    }

    #[test]
    fn sort_apps_orders_by_launch_count_then_name() {
        let mut app_a = test_app("Beta");
        app_a.launches = 2;
        let mut app_b = test_app("Alpha");
        app_b.launches = 5;
        let mut app_c = test_app("ActivityWatch");
        app_c.launches = 5;

        assert_eq!(
            sorted_names(vec![app_a, app_b, app_c]),
            vec!["ActivityWatch", "Alpha", "Beta"]
        );
    }

    #[test]
    fn sort_apps_keeps_hidden_apps_last() {
        let mut hidden = test_app("Hidden");
        hidden.hidden = true;
        hidden.launches = 100;
        let visible = test_app("Visible");

        assert_eq!(
            sorted_names(vec![hidden, visible]),
            vec!["Visible", "Hidden"]
        );
    }

    #[test]
    fn previous_metadata_falls_back_to_launch_target() {
        let mut old_app = test_app("App");
        old_app.id = "old-id".to_string();
        old_app.source = r"C:\Shortcuts\App.lnk".to_string();
        old_app.launches = 7;

        let mut new_app = old_app.clone();
        new_app.id = "new-id".to_string();
        new_app.source = r"C:\Apps\App\App.exe".to_string();
        new_app.launches = 0;

        let previous_metadata = HashMap::new();
        let previous_source_metadata = HashMap::new();
        let previous_launch_metadata =
            HashMap::from([(launch_metadata_key(&old_app), metadata_from_app(&old_app))]);

        let metadata = find_previous_metadata(
            &new_app,
            &previous_metadata,
            &previous_source_metadata,
            &previous_launch_metadata,
        )
        .expect("metadata should match by launch target");

        apply_metadata(&mut new_app, metadata);
        assert_eq!(new_app.launches, 7);
    }

    #[test]
    fn search_matches_english_fragments_and_initials() {
        let apps = vec![test_app("ActivityWatch"), test_app("Everything")];

        assert_eq!(matched_names(apps.clone(), "act"), vec!["ActivityWatch"]);
        assert_eq!(matched_names(apps.clone(), "wat"), vec!["ActivityWatch"]);
        assert_eq!(matched_names(apps, "aw"), vec!["ActivityWatch"]);
    }

    #[test]
    fn search_matches_chinese_pinyin_and_initials() {
        let apps = vec![test_app("微信"), test_app("网易云音乐")];

        assert_eq!(matched_names(apps.clone(), "wx"), vec!["微信"]);
        assert_eq!(matched_names(apps.clone(), "wei"), vec!["微信"]);
        assert_eq!(matched_names(apps.clone(), "xi"), vec!["微信"]);
        assert_eq!(matched_names(apps.clone(), "wyyyy"), vec!["网易云音乐"]);
        assert_eq!(matched_names(apps, "wang"), vec!["网易云音乐"]);
    }
}

fn lightweight_mode_enabled() -> bool {
    LIGHTWEIGHT_MODE.load(Ordering::Relaxed)
}

fn destroy_generation(label: &str) -> Option<&'static AtomicU64> {
    match label {
        MAIN_LABEL => Some(&MAIN_DESTROY_GENERATION),
        SETTINGS_LABEL => Some(&SETTINGS_DESTROY_GENERATION),
        _ => None,
    }
}

fn cancel_delayed_destroy(label: &str) {
    if let Some(generation) = destroy_generation(label) {
        generation.fetch_add(1, Ordering::Relaxed);
    }
}

fn dismiss_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        dismiss_webview_window(&window);
    }
}

fn dismiss_webview_window(window: &WebviewWindow) {
    if window.label() == MAIN_LABEL {
        let _ = window.emit("main-window-dismissed", ());
    }

    let _ = window.hide();
    if lightweight_mode_enabled() {
        schedule_delayed_destroy(window);
    }
}

fn schedule_delayed_destroy(window: &WebviewWindow) {
    let label = window.label().to_string();
    let Some(generation) = destroy_generation(&label) else {
        return;
    };
    let app = window.app_handle().clone();
    let expected_generation = generation.fetch_add(1, Ordering::Relaxed) + 1;

    thread::spawn(move || {
        thread::sleep(LIGHTWEIGHT_DESTROY_DELAY);
        let Some(generation) = destroy_generation(&label) else {
            return;
        };

        if generation.load(Ordering::Relaxed) != expected_generation {
            return;
        }

        if let Some(window) = app.get_webview_window(&label) {
            if !window.is_visible().unwrap_or(false) {
                let _ = window.destroy();
            }
        }
    });
}

fn schedule_retained_windows_destroy(app: &AppHandle) {
    for label in [MAIN_LABEL, SETTINGS_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            schedule_delayed_destroy(&window);
        }
    }
}

fn show_main_window(app: &AppHandle) {
    cancel_delayed_destroy(MAIN_LABEL);
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        position_main_window(&window);
        let _ = window.set_skip_taskbar(true);
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("focus-search", ());
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
            let _ = window.emit("focus-search", ());
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
    cancel_delayed_destroy(label);
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
            if MAIN_DIALOG_OPEN.load(Ordering::Relaxed) {
                return;
            }

            let close_window = close_window.clone();
            let was_focused = was_focused.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(120));
                if was_focused.load(Ordering::Relaxed)
                    && !MAIN_DIALOG_OPEN.load(Ordering::Relaxed)
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
    let scan_menu_item = SCAN_MENU_ITEM.get_or_init(|| Mutex::new(None));
    if let Ok(mut item) = scan_menu_item.lock() {
        *item = Some(scan.clone());
    }

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("TauriLaunch")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "about" => show_about_window(app),
            "scan" => spawn_scan(app.clone()),
            "settings" => show_settings_window(app),
            "lightweight" => {
                let checked = lightweight_for_event.is_checked().unwrap_or(false);
                LIGHTWEIGHT_MODE.store(checked, Ordering::Relaxed);
                if checked {
                    schedule_retained_windows_destroy(app);
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            create_tray(app)?;
            spawn_scan(app.handle().clone());
            let settings = load_settings().unwrap_or_default();
            if should_show_main_window_on_launch(&settings) {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dismiss_main_window,
            launch_app,
            hide_app,
            pin_app,
            reset_app_position,
            rename_app,
            open_app_directory,
            get_apps,
            scan_apps,
            set_main_dialog_open,
            search_apps,
            add_app,
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
