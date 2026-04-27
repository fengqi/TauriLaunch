use ib_pinyin::{matcher::PinyinMatcher, pinyin::PinyinNotation};
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
const DEFAULT_ICON_SIZE: u32 = 38;
const AUTOSTART_NAME: &str = "TauriLaunch";
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

fn default_icon_size() -> u32 {
    DEFAULT_ICON_SIZE
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
    #[serde(default = "default_icon_size")]
    icon_size: u32,
    #[serde(default)]
    autostart_enabled: bool,
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
}

#[derive(Debug, Clone, Default)]
struct AppMetadata {
    launches: u32,
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
    visible_apps(load_apps().unwrap_or_default())
}

#[tauri::command]
fn scan_apps(app: AppHandle) -> Result<Vec<AppEntry>, String> {
    scan_store_and_emit(&app).map_err(|error| error.to_string())
}

#[tauri::command]
fn search_apps(query: String) -> Vec<AppEntry> {
    filter_apps(visible_apps(load_apps().unwrap_or_default()), &query)
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

fn scan_store_and_emit(app: &AppHandle) -> io::Result<Vec<AppEntry>> {
    let apps = scan_configured_apps()?;
    store_apps(&apps)?;
    let visible = visible_apps(apps);
    let _ = app.emit("apps-updated", visible.clone());
    Ok(visible)
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

fn visible_apps(mut apps: Vec<AppEntry>) -> Vec<AppEntry> {
    sort_apps(&mut apps);
    apps.into_iter().filter(|app| !app.hidden).collect()
}

fn sort_apps(apps: &mut [AppEntry]) {
    apps.sort_by(|left, right| {
        right
            .launches
            .cmp(&left.launches)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
    });
}

fn launch_stored_app(app_id: &str) -> io::Result<Vec<AppEntry>> {
    let mut apps = load_apps()?;
    let Some(index) = apps.iter().position(|app| app.id == app_id && !app.hidden) else {
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
            Ok(visible_apps(apps))
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
    let Some(app) = apps.iter_mut().find(|app| app.id == app_id) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "应用不存在"));
    };

    app.hidden = true;
    sort_apps(&mut apps);
    store_apps(&apps)?;
    Ok(visible_apps(apps))
}

fn start_process(app: &AppEntry) -> io::Result<()> {
    let path = PathBuf::from(&app.path);
    let args = split_launch_args(&app.launch_args)?;
    let mut command = Command::new(&path);
    command.args(args);

    let working_dir = if app.working_dir.trim().is_empty() {
        path.parent().map(Path::to_path_buf)
    } else {
        Some(PathBuf::from(app.working_dir.trim()))
    };

    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    command.spawn().map(|_| ())
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
    let previous_metadata: HashMap<String, AppMetadata> = load_apps()?
        .into_iter()
        .map(|app| {
            (
                app.id,
                AppMetadata {
                    launches: app.launches,
                    hidden: app.hidden,
                    last_error: app.last_error,
                },
            )
        })
        .collect();
    let mut apps = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for directory in settings.watched_directories {
        let root = PathBuf::from(directory);
        if !root.is_dir() {
            continue;
        }
        scan_directory(&root, &previous_metadata, &mut seen, &mut apps)?;
    }

    sort_apps(&mut apps);
    Ok(apps)
}

fn scan_directory(
    root: &Path,
    previous_metadata: &HashMap<String, AppMetadata>,
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

                if let Some(metadata) = previous_metadata.get(&app_entry.id) {
                    app_entry.launches = metadata.launches;
                    app_entry.hidden = metadata.hidden;
                    app_entry.last_error = metadata.last_error.clone();
                }
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

fn powershell_single_quoted_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn build_app_entry(
    name: String,
    path: String,
    launch_args: String,
    working_dir: String,
    source: String,
) -> AppEntry {
    let id = stable_id(&source, &path, &launch_args);
    let initials = initials(&name);
    let search_text = search_text(&name, &path, &launch_args, &working_dir, &source, &initials);
    AppEntry {
        id: id.clone(),
        initials,
        search_text,
        icon_path: ensure_icon_cache(&id, &source, &path).unwrap_or_default(),
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

fn ensure_icon_cache(id: &str, source: &str, path: &str) -> io::Result<String> {
    let directory = icons_dir()?;
    fs::create_dir_all(&directory)?;
    let icon_path = directory.join(format!("{id}.png"));

    if icon_path.exists() {
        return Ok(icon_path.to_string_lossy().into_owned());
    }

    extract_icon_to_png(source, path, &icon_path)?;
    Ok(icon_path.to_string_lossy().into_owned())
}

fn extract_icon_to_png(source: &str, path: &str, output: &Path) -> io::Result<()> {
    let source = powershell_single_quoted_text(source);
    let path = powershell_single_quoted_text(path);
    let output = powershell_single_quoted(output);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$outputPath = {output}
$candidates = @({source}, {path})
$icon = $null
foreach ($candidate in $candidates) {{
  if ([string]::IsNullOrWhiteSpace($candidate) -or -not (Test-Path -LiteralPath $candidate)) {{
    continue
  }}
  try {{
    $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($candidate)
    if ($null -ne $icon) {{
      break
    }}
  }} catch {{}}
}}
if ($null -eq $icon) {{
  exit 1
}}
$bitmap = $icon.ToBitmap()
$bitmap.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap.Dispose()
$icon.Dispose()
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
        .plugin(tauri_plugin_dialog::init())
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
            launch_app,
            hide_app,
            get_apps,
            scan_apps,
            search_apps,
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
