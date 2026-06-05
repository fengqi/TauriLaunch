# AGENTS.md — TauriLaunch 代码地图

> 入口文档，包含关键文件位置和架构骨架。详细规则见 `docs/`。

## 项目概览

Windows 软件启动器，Tauri v2 (Rust) + React 19 + TypeScript + Vite。常驻系统托盘，扫描监听目录中的 `.exe` / `.lnk`，支持拼音搜索、图标缓存、启动统计。

## 快速导航

| 文档 | 用途 |
|------|------|
| `docs/PRODUCT_PLAN.md` | 产品功能、交互规则、验收标准 |
| `docs/DECISIONS.md` | 技术决策、架构边界 |
| `docs/DEV_NOTES.md` | 开发命令、数据格式、实现细节 |

## 关键文件

| 文件 | 行数 | 职责 |
|------|------|------|
| `src-tauri/src/lib.rs` | ~2610 | **全部后端逻辑**（20 个 Tauri command + 扫描/启动/图标/设置/窗口） |
| `src-tauri/src/main.rs` | ~5 | Windows 子系统入口 |
| `src-tauri/Cargo.toml` | ~29 | Rust 依赖 |
| `src-tauri/tauri.conf.json` | - | Tauri 配置、窗口权限 |
| `src-tauri/capabilities/default.json` | - | IPC 权限 |
| `src/App.tsx` | ~1440 | **全部前端**（LauncherWindow / SettingsWindow / AboutWindow） |
| `src/App.css` | - | 全局样式 |
| `index.html` | - | 前端入口 |
| `package.json` | - | npm 配置 |

## 后端架构 (`src-tauri/src/lib.rs`)

### 数据结构

| 结构体 | 行号 | 说明 |
|--------|------|------|
| `LaunchMode` (enum) | 58 | `Tray` / `Window` |
| `AppSettings` | 98 | 所有设置项，序列化为 `settings.json` |
| `AppEntry` | 152 | 应用条目，序列化为 `apps.json` |
| `ShortcutInfo` | 178 | `.lnk` 解析结果 |
| `StartAppInfo` | 190 | 开始菜单应用 |
| `AppMetadata` | 197 | 可继承的条目元数据（启动次数、自定义名称等） |

### 全局状态 (statics)

| 变量 | 行号 | 用途 |
|------|------|------|
| `LIGHTWEIGHT_MODE` | 46 | 轻量模式开关 |
| `SCAN_RUNNING` | 47 | 防并发扫描 |
| `ICON_CACHE_REBUILD_RUNNING` | 48 | 防并发图标重建 |
| `MAIN_DIALOG_OPEN` | 49 | 文件选择框打开标记 |
| `MAIN_DESTROY_GENERATION` | 50 | 主窗口销毁代际 |
| `SETTINGS_DESTROY_GENERATION` | 51 | 设置窗口销毁代际 |
| `SCAN_MENU_ITEM` | 52 | 托盘菜单”扫描“项引用 |
| `WATCHER_GENERATION` | 53 | 目录监控代际（重启时递增） |
| `WATCH_TRIGGER` | 54 | 目录监控防抖计数器 |

### Tauri Commands（20 个）

| Command | 行号 | 功能 |
|---------|------|------|
| `get_settings` | 213 | 读取设置 |
| `save_settings` | 221 | 保存设置，目录变化时重启 watcher |
| `get_apps` | 238 | 返回可见应用列表 |
| `scan_apps` | 243 | 触发后台扫描 |
| `rebuild_icon_cache` | 248 | 重建全部图标缓存 |
| `rebuild_single_icon_cache` | 253 | 重建单个图标缓存 |
| `set_main_dialog_open` | 258 | 标记文件选择框状态 |
| `search_apps` | 263 | 搜索过滤（含拼音） |
| `add_app` | 268 | 手动添加应用 |
| `launch_app` | 273 | 启动应用（含 UAC 提权） |
| `hide_app` | 281 | 隐藏/删除应用 |
| `pin_app` | 288 | 置顶应用 |
| `reset_app_position` | 295 | 重置应用排序 |
| `rename_app` | 302 | 修改自定义名称 |
| `open_app_directory` | 309 | 打开应用所在目录 |
| `dismiss_main_window` | 314 | 关闭主窗口 |
| `close_settings_window` | 319 | 关闭设置窗口 |

### 核心流程

#### 扫描流程
```
scan_apps (243)
  → spawn_scan (398)
    → [后台线程] scan_store_and_emit (503)
      → scan_configured_apps (1082)  // 遍历监听目录
        → scan_directory (1132)      // 扫描单层 .exe/.lnk（不递归）
        → scan_start_apps (1210)     // PowerShell Get-StartApps
      → store_apps (388)             // 写入 apps.json
      → emit_apps_updated_to_visible_main (559)  // 主窗口可见时推送
```

#### 目录监控流程
```
run() setup (2573)
  → start_directory_watcher (415)
    → notify::recommended_watcher 监听所有目录 (NonRecursive)
    → 收到事件 → WATCH_TRIGGER 递增 → 30s 后若计数器未变 → spawn_scan

save_settings (221)
  → 检测 watched_directories 变化
  → restart_directory_watcher (483) → 递增 WATCHER_GENERATION
```

#### 启动流程
```
launch_app (273)
  → launch_stored_app (743)
  → start_process (953)         // Command::spawn
    → 失败 740 → shell_execute (1001)  // ShellExecuteW runas UAC
  → emit apps-updated
  → dismiss_main_window
```

#### 窗口生命周期
```
show_main_window (2366) → 存在则显示，否则创建
dismiss_window (2315)
  → 轻量模式: dismiss_webview_window (2321) → 隐藏 + schedule_delayed_destroy (2332, 60s)
  → 非轻量模式: 隐藏（保留 WebView）
```

### 持久化路径
- `%LOCALAPPDATA%\TauriLaunch\settings.json`
- `%LOCALAPPDATA%\TauriLaunch\apps.json`
- `%LOCALAPPDATA%\TauriLaunch\icons\{id}-128.png`

## 前端架构 (`src/App.tsx`)

三个组件通过 URL query `?view=` 路由：
- `LauncherWindow` (169) — 主界面：搜索框、6列网格、扫描按钮、右键菜单
- `SettingsWindow` (916) — 五个标签页：目录监听/图标/界面/其他/信息
- `AboutWindow` (1421) — 版本信息

### 核心状态
- `apps: AppEntry[]` — 应用列表
- `query` — 搜索关键词
- `scanning` — 扫描进行中
- `settings: AppSettings` — 当前设置

### 事件监听
- `apps-updated` → 刷新应用列表
- `scan-state-changed` → 更新扫描按钮状态
- `scan-failed` → 显示错误
- `settings-updated` → 同步设置
- `focus-search` → 聚焦搜索框
- `icon-cache-rebuild-finished` → 刷新图标

### IPC 调用
前端通过 `invoke()` 调用 Tauri commands，更新通过 `listen()` 接收事件。

## 常用命令

```powershell
npm install
npm run tauri dev      # 开发模式
npm run tauri build    # 生产构建
```

Rust 检查: `cargo check` (在 `src-tauri/` 下)
