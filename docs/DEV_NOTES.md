# TauriLaunch 开发说明

## 当前环境

- Windows 开发。
- Rust/Cargo/rustup 已安装。
- Cargo crates.io 源已配置为清华大学镜像。
- Visual Studio 2022 Build Tools 已安装。
- MSVC `cl.exe` 已验证可用。
- Windows SDK `rc.exe`、`mt.exe` 已验证可用。
- WebView2 Runtime 已存在。
- Node.js：`v24.14.0`。
- npm：`11.9.0`。

## 常用命令

```powershell
npm install
npm run dev
npm run tauri dev
npm run build
npm run tauri build
```

如果当前 PowerShell 找不到 Rust，可临时补 PATH：

```powershell
$env:Path="$env:USERPROFILE\.cargo\bin;$env:Path"
```

如果当前 PowerShell 找不到 MSVC 工具，可使用 Developer Command Prompt，或先加载：

```powershell
$vsdev = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat'
cmd /c "call `"$vsdev`" -arch=x64 && npm run tauri dev"
```

## 设计参考

- 主界面：`docs/design/launcher-main.png`
- 设置窗口：`docs/design/settings-dialog.png`
- 托盘菜单：`docs/design/tray-context-menu.png`

## 本地数据

配置文件：

```text
%LOCALAPPDATA%\TauriLaunch\settings.json
```

当前字段：

```json
{
  "windowRightOffset": 10,
  "windowBottomOffset": 10,
  "startupLaunchMode": "tray",
  "manualLaunchMode": "window",
  "iconSize": 38,
  "autostartEnabled": false,
  "physicalDeleteEnabled": false,
  "showHiddenApps": false,
  "watchedDirectories": []
}
```

应用列表：

```text
%LOCALAPPDATA%\TauriLaunch\apps.json
```

当前字段：

```json
[
  {
    "id": "stable-id",
    "name": "AppName",
    "path": "C:\\Path\\App.exe",
    "launchArgs": "",
    "workingDir": "C:\\Path",
    "launches": 0,
    "accent": "#2563eb",
    "initials": "AP",
    "searchText": "appname app ap c:\\path\\app.exe",
    "iconPath": "C:\\Users\\fengqi\\AppData\\Local\\TauriLaunch\\icons\\stable-id-128.png",
    "customName": "",
    "source": "C:\\Shortcut\\App.lnk",
    "hidden": false,
    "lastError": ""
  }
]
```

## 注意事项

- 主窗口每次创建或重新显示时都会按设置中的右边距、下边距定位到屏幕右下角。
- 设置窗口和关于窗口创建时直接居中显示。
- 开机启动通过 `--startup` 或 `--autostart` 参数识别，默认只启动到托盘。
- 普通双击启动默认显示主窗口，可在设置中改为启动到托盘。
- 托盘菜单 `退出` 才真正退出整个进程。
- 托盘 `轻量模式` 只在当前进程内生效，不保存到 JSON；轻量模式关闭/失焦时先隐藏窗口，60 秒后仍未重新打开才销毁 WebView。
- 开机启动设置写入当前用户 `Run` 注册表项，启动参数为 `--startup`。
- 图标大小可选 `32`、`38`、`48`，设置窗口不提供 `64`。
- 图标缓存路径为 `%LOCALAPPDATA%\TauriLaunch\icons\{id}-128.png`；应用商店 / MSIX 条目使用 `{id}-shell-128.png`，用于区分按 Appx 清单生成的裁边图标。
- 图标提取优先通过 Windows `PrivateExtractIcons` 请求 128、64、48、32 档图标；失败时再回退到 `ExtractAssociatedIcon`。
- 当前 `.lnk` 解析使用 PowerShell 调用 Windows WScript Shell COM，后续如果扫描性能不够，再替换为 Rust 侧直接 COM 调用。
- `.lnk` 解析后必须确认目标文件存在；目标不存在的残留快捷方式不生成条目，也不进入图标提取。
- 应用商店 / MSIX 应用不依赖监听目录快捷方式；扫描阶段调用 `Get-StartApps`，把带 `AppID` 的项目保存为 `shell:AppsFolder\...`，再通过 Appx 包清单里的 `Square150x150Logo` / `Square44x44Logo` 生成 128 图标缓存。
- 扫描先按 `source` 查旧条目；旧条目目标文件存在、工作目录有效或为空、启动参数可解析、图标缓存路径是当前 `{id}-128.png` 格式且文件存在时，直接复用旧记录。需要重建条目时，旧 metadata 按 `id`、`source`、实际启动目标加启动参数依次匹配，避免来源变化导致启动次数清零。旧 `{id}-256.png` 数据会在下次扫描时被视为旧格式并重新生成 128 图标。
- 图标缓存缺失、图标路径是旧格式、目标不存在、工作目录失效或启动参数不可解析时，不复用旧记录，重新解析 `.lnk` / `.exe` 并重新尝试图标提取。
- `scan_apps` 只启动后台扫描线程并立即返回，不直接返回应用列表；托盘扫描和启动扫描也走同一后台扫描入口。
- 后台扫描由进程内 `SCAN_RUNNING` 防重复；已有扫描运行时再次触发扫描会直接忽略。
- 后台扫描开始和结束会发送 `scan-state-changed` 事件；扫描运行时托盘 `扫描` 菜单项通过保存的 `MenuItem` 置灰，主界面扫描按钮显示 spinner。
- 后台扫描完成后写入 `apps.json`；只有主窗口可见时才向主窗口发送 `apps-updated`。隐藏窗口或轻量模式销毁 WebView 时不主动刷新，下一次打开由 `get_apps` 读取最新数据。
- 后台扫描失败时只在主窗口可见时发送 `scan-failed`，前端收到后显示错误。
- `initials` 和 `searchText` 在 Rust 扫描阶段生成；`searchText` 只保存普通文本、路径、英文分词和英文首字母。
- 中文拼音搜索由 Rust 后端 `search_apps` 命令使用 `ib-pinyin` 执行，前端不要重复计算拼音索引。
- 真实启动由 Rust 后端 `launch_app` 执行，使用条目中的路径、启动参数和工作目录。
- `launch_app` 先用 `Command::spawn()` 启动；如果 Windows 返回 raw os error `740`，再用 `ShellExecuteW` + `runas` 重试，让系统弹 UAC。
- `shell:AppsFolder\...` 条目直接用 `ShellExecuteW` + `open` 启动，不走 `Command::spawn()`。
- 后端返回应用列表前统一排序：启动次数从大到小，次数相同按名称排序。
- 前端单击应用条目启动应用，启动成功后清空搜索状态并刷新列表。
- 主窗口创建或重新显示后，后端发送 `focus-search` 事件，前端聚焦搜索输入框；前端初次挂载时也主动聚焦一次。
- 前端 `添加` 使用系统文件选择框选择单个 `.exe` 或 `.lnk`；打开选择框前调用 `set_main_dialog_open(true)`，结束后恢复为 `false`，避免主窗口失焦自动隐藏。
- 后端 `add_app` 复用 `.exe` / `.lnk` 构造逻辑；已有可见条目不重复添加，隐藏条目恢复显示并将 `launches` 置为 0。
- 前端搜索框有清空按钮；点击关闭、失焦隐藏、启动应用、后端通知主窗口即将隐藏或销毁时都清空搜索状态。
- 前端右键应用条目显示自定义菜单，并屏蔽 WebView 默认右键菜单。
- 修改名称写入 `customName`，扫描时保留；只重建显示名称、首字母和搜索索引，不修改路径、启动参数和工作目录。
- 条目隐藏由 Rust 后端 `hide_app` 执行，隐藏状态写入 `apps.json` 并在重新扫描时保留。
- `settings.json` 的 `physicalDeleteEnabled` 控制删除策略：默认 `false` 表示隐藏；为 `true` 时 `hide_app` 会移除本地记录并删除当前图标缓存。
- `settings.json` 的 `showHiddenApps` 控制隐藏条目是否返回给前端；启用后后端排序仍保证隐藏条目在可见条目之后。
- 当前主界面“添加”是保留入口，真实功能在后续阶段接入。
- 用户要求调整功能、交互、产品规则时，同步更新文档中的最终版本描述。
- 纯 bug 修复不用更新产品计划。
- 文档不要使用临时补充段落堆叠历史，只保留当前有效规则。
