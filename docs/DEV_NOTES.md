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

如果当前 PowerShell 找不到 MSVC 工具，可用 Developer Command Prompt，或先加载：

```powershell
$vsdev = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat'
cmd /c "call `"$vsdev`" -arch=x64 && npm run tauri dev"
```

## 设计参考

- 主界面：`docs/design/launcher-main.png`
- 设置窗口：`docs/design/settings-dialog.png`
- 托盘菜单：`docs/design/tray-context-menu.png`

## 注意事项

- 主窗口关闭不是普通隐藏，而是销毁 WebView 窗口。
- 失焦销毁只针对主启动器窗口，不用于设置窗口和关于窗口。
- 主窗口每次创建/显示时都会按设置中的右边距、下边距定位到屏幕右下角。
- 托盘菜单 `退出` 才真正退出整个进程。
- M1 中扫描和启动是真功能的占位入口，后续在 M2/M3 补齐。
- 用户要求调整功能、交互、产品规则时，同步更新文档中的最终版本描述。
- 纯 bug 修复不用更新产品计划。
- 文档不要用临时补充段落堆叠历史，只保留当前有效规则。

## 窗口位置配置

主窗口位置配置保存在：

```text
%LOCALAPPDATA%\com.fengqi.taurilaunch\settings.json
```

字段：

```json
{
  "windowRightOffset": 10,
  "windowBottomOffset": 10
}
```

主窗口创建或重新显示时会读取该文件，并按当前显示器可用区域计算右下角位置。
