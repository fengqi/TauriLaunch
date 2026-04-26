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
%LOCALAPPDATA%\com.fengqi.taurilaunch\settings.json
```

当前字段：

```json
{
  "windowRightOffset": 10,
  "windowBottomOffset": 10,
  "startupLaunchMode": "tray",
  "manualLaunchMode": "window",
  "watchedDirectories": [
    "C:\\Users\\fengqi\\Desktop\\App",
    "C:\\Users\\fengqi\\Desktop\\Game",
    "C:\\Users\\fengqi\\Desktop\\SingleExe"
  ]
}
```

应用列表：

```text
%LOCALAPPDATA%\com.fengqi.taurilaunch\apps.json
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
    "source": "C:\\Shortcut\\App.lnk"
  }
]
```

## 注意事项

- 主窗口每次创建或重新显示时都会按设置中的右边距、下边距定位到屏幕右下角。
- 设置窗口和关于窗口创建时直接居中显示。
- 开机启动通过 `--startup` 或 `--autostart` 参数识别，默认只启动到托盘。
- 普通双击启动默认显示主窗口，可在设置中改为启动到托盘。
- 托盘菜单 `退出` 才真正退出整个进程。
- 托盘 `轻量模式` 只在当前进程内生效，不保存到 JSON。
- 当前 `.lnk` 解析使用 PowerShell 调用 Windows WScript Shell COM，后续如果扫描性能不够，再替换为 Rust 侧直接 COM 调用。
- `initials` 和 `searchText` 在 Rust 扫描阶段生成；`searchText` 只保存普通文本、路径、英文分词和英文首字母。
- 中文拼音搜索由 Rust 后端 `search_apps` 命令使用 `ib-pinyin` 执行，前端不要重复计算拼音索引。
- 当前主界面“添加”和设置里的“浏览”是保留入口，真实功能在后续阶段接入。
- 用户要求调整功能、交互、产品规则时，同步更新文档中的最终版本描述。
- 纯 bug 修复不用更新产品计划。
- 文档不要使用临时补充段落堆叠历史，只保留当前有效规则。
