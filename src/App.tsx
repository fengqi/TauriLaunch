import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, FocusEvent, KeyboardEvent, MouseEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type AppEntry = {
  id: string;
  name: string;
  path: string;
  launchArgs: string;
  launches: number;
  accent: string;
  initials: string;
};

type TooltipState = {
  app: AppEntry;
  anchor: {
    left: number;
    right: number;
    top: number;
    bottom: number;
  };
};

type LaunchMode = "tray" | "window";

type AppSettings = {
  windowRightOffset: number;
  windowBottomOffset: number;
  startupLaunchMode: LaunchMode;
  manualLaunchMode: LaunchMode;
};

type SettingsTab = "directories" | "icons" | "interface" | "other" | "info";

const defaultSettings: AppSettings = {
  windowRightOffset: 10,
  windowBottomOffset: 10,
  startupLaunchMode: "tray",
  manualLaunchMode: "window",
};

const sampleApps: AppEntry[] = [
  {
    id: "steam",
    name: "Steam",
    path: "C:\\Program Files\\Steam\\Steam.exe",
    launchArgs: "",
    launches: 35,
    accent: "#245b9f",
    initials: "St",
  },
  {
    id: "wechat",
    name: "微信",
    path: "C:\\Program Files\\Tencent\\WeChat\\WeChat.exe",
    launchArgs: "",
    launches: 18,
    accent: "#28c76f",
    initials: "微",
  },
  {
    id: "notepad2",
    name: "Notepad2",
    path: "C:\\Tools\\Notepad2\\Notepad2.exe",
    launchArgs: "",
    launches: 9,
    accent: "#8bb8c8",
    initials: "N2",
  },
  {
    id: "uu",
    name: "UU加速器",
    path: "C:\\Program Files\\Netease\\UU\\uu.exe",
    launchArgs: "",
    launches: 4,
    accent: "#06a6d8",
    initials: "UU",
  },
  {
    id: "kook",
    name: "KOOK",
    path: "C:\\Users\\fengqi\\AppData\\Local\\KOOK\\KOOK.exe",
    launchArgs: "",
    launches: 12,
    accent: "#64d923",
    initials: "K",
  },
  {
    id: "vscode",
    name: "Visual Studio Code",
    path: "C:\\Users\\fengqi\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
    launchArgs: "--reuse-window",
    launches: 41,
    accent: "#2f80ed",
    initials: "VS",
  },
  {
    id: "everything",
    name: "Everything",
    path: "C:\\Program Files\\Everything\\Everything.exe",
    launchArgs: "-startup",
    launches: 28,
    accent: "#ff8a00",
    initials: "Ev",
  },
  {
    id: "telegram",
    name: "Telegram",
    path: "C:\\Users\\fengqi\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe",
    launchArgs: "",
    launches: 22,
    accent: "#2aabee",
    initials: "T",
  },
  {
    id: "obs",
    name: "Neat Download Manager",
    path: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe",
    launchArgs: "",
    launches: 7,
    accent: "#303238",
    initials: "OBS",
  },
  {
    id: "potplayer",
    name: "Driver Store Explorer",
    path: "C:\\Program Files\\DAUM\\PotPlayer\\PotPlayerMini64.exe",
    launchArgs: "",
    launches: 16,
    accent: "#ffd400",
    initials: "P",
  },
  {
    id: "steam",
    name: "Kingdom Come - Deliverance II",
    path: "C:\\Program Files\\Steam\\Steam.exe",
    launchArgs: "",
    launches: 35,
    accent: "#245b9f",
    initials: "St",
  },
  {
    id: "wechat",
    name: "微信",
    path: "C:\\Program Files\\Tencent\\WeChat\\WeChat.exe",
    launchArgs: "",
    launches: 18,
    accent: "#28c76f",
    initials: "微",
  },
  {
    id: "notepad2",
    name: "Notepad2",
    path: "C:\\Tools\\Notepad2\\Notepad2.exe",
    launchArgs: "",
    launches: 9,
    accent: "#8bb8c8",
    initials: "N2",
  },
  {
    id: "uu",
    name: "UU加速器",
    path: "C:\\Program Files\\Netease\\UU\\uu.exe",
    launchArgs: "",
    launches: 4,
    accent: "#06a6d8",
    initials: "UU",
  },
  {
    id: "kook",
    name: "KOOK",
    path: "C:\\Users\\fengqi\\AppData\\Local\\KOOK\\KOOK.exe",
    launchArgs: "",
    launches: 12,
    accent: "#64d923",
    initials: "K",
  },
  {
    id: "vscode",
    name: "VS Code",
    path: "C:\\Users\\fengqi\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
    launchArgs: "--reuse-window",
    launches: 41,
    accent: "#2f80ed",
    initials: "VS",
  },
  {
    id: "everything",
    name: "Everything",
    path: "C:\\Program Files\\Everything\\Everything.exe",
    launchArgs: "-startup",
    launches: 28,
    accent: "#ff8a00",
    initials: "Ev",
  },
  {
    id: "telegram",
    name: "Telegram",
    path: "C:\\Users\\fengqi\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe",
    launchArgs: "",
    launches: 22,
    accent: "#2aabee",
    initials: "T",
  },
  {
    id: "obs",
    name: "OBS Studio",
    path: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe",
    launchArgs: "",
    launches: 7,
    accent: "#303238",
    initials: "OBS",
  },
  {
    id: "potplayer",
    name: "PotPlayer",
    path: "C:\\Program Files\\DAUM\\PotPlayer\\PotPlayerMini64.exe",
    launchArgs: "",
    launches: 16,
    accent: "#ffd400",
    initials: "P",
  },
  {
    id: "steam",
    name: "Steam",
    path: "C:\\Program Files\\Steam\\Steam.exe",
    launchArgs: "",
    launches: 35,
    accent: "#245b9f",
    initials: "St",
  },
  {
    id: "wechat",
    name: "微信",
    path: "C:\\Program Files\\Tencent\\WeChat\\WeChat.exe",
    launchArgs: "",
    launches: 18,
    accent: "#28c76f",
    initials: "微",
  },
  {
    id: "notepad2",
    name: "Notepad2",
    path: "C:\\Tools\\Notepad2\\Notepad2.exe",
    launchArgs: "",
    launches: 9,
    accent: "#8bb8c8",
    initials: "N2",
  },
  {
    id: "uu",
    name: "UU加速器",
    path: "C:\\Program Files\\Netease\\UU\\uu.exe",
    launchArgs: "",
    launches: 4,
    accent: "#06a6d8",
    initials: "UU",
  },
  {
    id: "kook",
    name: "KOOK",
    path: "C:\\Users\\fengqi\\AppData\\Local\\KOOK\\KOOK.exe",
    launchArgs: "",
    launches: 12,
    accent: "#64d923",
    initials: "K",
  },
  {
    id: "vscode",
    name: "VS Code",
    path: "C:\\Users\\fengqi\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
    launchArgs: "--reuse-window",
    launches: 41,
    accent: "#2f80ed",
    initials: "VS",
  },
  {
    id: "everything",
    name: "Everything",
    path: "C:\\Program Files\\Everything\\Everything.exe",
    launchArgs: "-startup",
    launches: 28,
    accent: "#ff8a00",
    initials: "Ev",
  },
  {
    id: "telegram",
    name: "Telegram",
    path: "C:\\Users\\fengqi\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe",
    launchArgs: "",
    launches: 22,
    accent: "#2aabee",
    initials: "T",
  },
  {
    id: "obs",
    name: "OBS Studio",
    path: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe",
    launchArgs: "",
    launches: 7,
    accent: "#303238",
    initials: "OBS",
  },
  {
    id: "potplayer",
    name: "PotPlayer",
    path: "C:\\Program Files\\DAUM\\PotPlayer\\PotPlayerMini64.exe",
    launchArgs: "",
    launches: 16,
    accent: "#ffd400",
    initials: "P",
  },
  {
    id: "steam",
    name: "Steam",
    path: "C:\\Program Files\\Steam\\Steam.exe",
    launchArgs: "",
    launches: 35,
    accent: "#245b9f",
    initials: "St",
  },
  {
    id: "wechat",
    name: "微信",
    path: "C:\\Program Files\\Tencent\\WeChat\\WeChat.exe",
    launchArgs: "",
    launches: 18,
    accent: "#28c76f",
    initials: "微",
  },
  {
    id: "notepad2",
    name: "Notepad2",
    path: "C:\\Tools\\Notepad2\\Notepad2.exe",
    launchArgs: "",
    launches: 9,
    accent: "#8bb8c8",
    initials: "N2",
  },
  {
    id: "uu",
    name: "UU加速器",
    path: "C:\\Program Files\\Netease\\UU\\uu.exe",
    launchArgs: "",
    launches: 4,
    accent: "#06a6d8",
    initials: "UU",
  },
  {
    id: "kook",
    name: "KOOK",
    path: "C:\\Users\\fengqi\\AppData\\Local\\KOOK\\KOOK.exe",
    launchArgs: "",
    launches: 12,
    accent: "#64d923",
    initials: "K",
  },
  {
    id: "vscode",
    name: "VS Code",
    path: "C:\\Users\\fengqi\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
    launchArgs: "--reuse-window",
    launches: 41,
    accent: "#2f80ed",
    initials: "VS",
  },
  {
    id: "everything",
    name: "Everything",
    path: "C:\\Program Files\\Everything\\Everything.exe",
    launchArgs: "-startup",
    launches: 28,
    accent: "#ff8a00",
    initials: "Ev",
  },
  {
    id: "telegram",
    name: "Telegram",
    path: "C:\\Users\\fengqi\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe",
    launchArgs: "",
    launches: 22,
    accent: "#2aabee",
    initials: "T",
  },
  {
    id: "obs",
    name: "OBS Studio",
    path: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe",
    launchArgs: "",
    launches: 7,
    accent: "#303238",
    initials: "OBS",
  },
  {
    id: "potplayer",
    name: "PotPlayer",
    path: "C:\\Program Files\\DAUM\\PotPlayer\\PotPlayerMini64.exe",
    launchArgs: "",
    launches: 16,
    accent: "#ffd400",
    initials: "P",
  },
];

const directories = [
  "C:\\Users\\fengqi\\Desktop\\App",
  "C:\\Users\\fengqi\\Desktop\\Game",
  "C:\\Users\\fengqi\\Desktop\\SingleExe",
];

const settingsTabs: { id: SettingsTab; label: string }[] = [
  { id: "directories", label: "目录监听" },
  { id: "icons", label: "图标设置" },
  { id: "interface", label: "界面设置" },
  { id: "other", label: "其他设置" },
  { id: "info", label: "信息" },
];

function App() {
  const view = new URLSearchParams(window.location.search).get("view") ?? "main";

  if (view === "settings") {
    return <SettingsWindow />;
  }

  if (view === "about") {
    return <AboutWindow />;
  }

  return <LauncherWindow />;
}

function LauncherWindow() {
  const [query, setQuery] = useState("");
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({
    left: 0,
    top: 0,
    visibility: "hidden",
  });
  const tooltipRef = useRef<HTMLDivElement>(null);

  const apps = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword) {
      return sampleApps;
    }

    return sampleApps.filter(
      (app) =>
        app.name.toLowerCase().includes(keyword) ||
        app.path.toLowerCase().includes(keyword),
    );
  }, [query]);

  useLayoutEffect(() => {
    if (!tooltip || !tooltipRef.current) {
      return;
    }

    const margin = 8;
    const gap = 8;
    const tooltipWidth = tooltipRef.current.offsetWidth;
    const tooltipHeight = tooltipRef.current.offsetHeight;
    let left = tooltip.anchor.right + gap;
    let top = tooltip.anchor.bottom + gap;

    if (left + tooltipWidth > window.innerWidth - margin) {
      left = tooltip.anchor.left - tooltipWidth - gap;
    }

    if (left < margin) {
      left = Math.max(margin, window.innerWidth - tooltipWidth - margin);
    }

    if (top + tooltipHeight > window.innerHeight - margin) {
      top = tooltip.anchor.top - tooltipHeight - gap;
    }

    if (top < margin) {
      top = margin;
    }

    setTooltipStyle({
      left,
      top,
      visibility: "visible",
    });
  }, [tooltip]);

  async function closeMainWindow() {
    await invoke("dismiss_main_window");
  }

  async function launchApp(app: AppEntry) {
    await invoke("dismiss_after_launch", { appName: app.name });
  }

  function handleSearchKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" && apps.length > 0) {
      event.preventDefault();
      void launchApp(apps[0]);
    }
  }

  function showAppTooltip(
    app: AppEntry,
    event: MouseEvent<HTMLButtonElement> | FocusEvent<HTMLButtonElement>,
  ) {
    const rect = event.currentTarget.getBoundingClientRect();
    setTooltipStyle({
      left: 0,
      top: 0,
      visibility: "hidden",
    });
    setTooltip({
      app,
      anchor: {
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
      },
    });
  }

  function hideAppTooltip() {
    setTooltip(null);
  }

  return (
    <main className="launcher-shell">
      <header className="launcher-header">
        <div className="title-block">
          <div className="app-mark" aria-hidden="true">
            <span />
            <span />
            <span />
            <span />
            <span />
            <span />
            <span />
            <span />
            <span />
          </div>
          <span>应用列表</span>
        </div>
        <div className="toolbar">
          <label className="search-box">
            <span aria-hidden="true">🔍</span>
            <input
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder="搜索应用"
            />
          </label>
          <button type="button" className="toolbar-button">
            <span aria-hidden="true">＋</span>
            添加
          </button>
          <button
            type="button"
            className="toolbar-button"
            onClick={closeMainWindow}
          >
            <span aria-hidden="true">×</span>
            关闭
          </button>
        </div>
      </header>

      <section className="app-grid" aria-label="应用列表">
        {apps.map((app, index) => (
          <button
            key={`${app.id}-${index}`}
            type="button"
            className="app-tile"
            onDoubleClick={() => void launchApp(app)}
            onMouseEnter={(event) => showAppTooltip(app, event)}
            onMouseLeave={hideAppTooltip}
            onFocus={(event) => showAppTooltip(app, event)}
            onBlur={hideAppTooltip}
          >
            <span className="app-icon" style={{ background: app.accent }}>
              {app.initials}
            </span>
            <span className="app-name">{app.name}</span>
          </button>
        ))}
      </section>

      {tooltip ? (
        <div
          ref={tooltipRef}
          className="app-tooltip-floating"
          style={tooltipStyle}
        >
          <span>启动次数: {tooltip.app.launches}</span>
          <span>路径: {tooltip.app.path}</span>
          {tooltip.app.launchArgs ? (
            <span>启动参数: {tooltip.app.launchArgs}</span>
          ) : null}
        </div>
      ) : null}
    </main>
  );
}

function SettingsWindow() {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [activeTab, setActiveTab] = useState<SettingsTab>("directories");
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    let canceled = false;

    invoke<AppSettings>("get_settings")
      .then((loaded) => {
        if (!canceled) {
          setSettings(loaded);
        }
      })
      .catch(() => {
        if (!canceled) {
          setSettings(defaultSettings);
        }
      });

    return () => {
      canceled = true;
    };
  }, []);

  async function saveAndClose() {
    try {
      setSaveError("");
      await invoke("save_settings", { settings });
      await invoke("close_settings_window");
    } catch (error) {
      setSaveError(String(error));
    }
  }

  async function closeSettings() {
    await invoke("close_settings_window");
  }

  function updateNumberSetting(
    name: "windowRightOffset" | "windowBottomOffset",
    value: string,
  ) {
    setSettings((current) => ({
      ...current,
      [name]: Math.max(0, Number.parseInt(value, 10) || 0),
    }));
  }

  function updateLaunchMode(
    name: "startupLaunchMode" | "manualLaunchMode",
    value: string,
  ) {
    if (value !== "tray" && value !== "window") {
      return;
    }

    setSettings((current) => ({
      ...current,
      [name]: value,
    }));
  }

  function renderSettingsPanel() {
    if (activeTab === "directories") {
      return (
        <div className="settings-section directory-section">
          <h2>目录监听</h2>
          <div className="directory-list">
            {directories.map((directory) => (
              <div key={directory}>{directory}</div>
            ))}
          </div>
          <div className="directory-actions">
            <input aria-label="目录路径" />
            <button type="button">浏览</button>
            <button type="button">添加</button>
            <button type="button">删除</button>
          </div>
        </div>
      );
    }

    if (activeTab === "icons") {
      return (
        <div className="settings-section compact-row">
          <h2>图标设置</h2>
          <label>
            <span>图标大小</span>
            <select defaultValue="32x32">
              <option>32x32</option>
              <option>48x48</option>
              <option>64x64</option>
            </select>
          </label>
        </div>
      );
    }

    if (activeTab === "interface") {
      return (
        <div className="settings-section form-grid">
          <h2>界面设置</h2>
          <label>
            <span>窗口位置与屏幕右边距离</span>
            <input
              type="number"
              min={0}
              value={settings.windowRightOffset}
              onChange={(event) =>
                updateNumberSetting(
                  "windowRightOffset",
                  event.currentTarget.value,
                )
              }
            />
          </label>
          <label>
            <span>窗口位置与屏幕下边距离</span>
            <input
              type="number"
              min={0}
              value={settings.windowBottomOffset}
              onChange={(event) =>
                updateNumberSetting(
                  "windowBottomOffset",
                  event.currentTarget.value,
                )
              }
            />
          </label>
          <label className="checkbox-row">
            <span>开启实时搜索功能</span>
            <input type="checkbox" defaultChecked />
          </label>
          <label>
            <span>搜索执行延迟(毫秒)</span>
            <input type="number" defaultValue={1000} />
          </label>
          <label className="checkbox-row">
            <span>搜索后回车打开首个</span>
            <input type="checkbox" defaultChecked />
          </label>
        </div>
      );
    }

    if (activeTab === "other") {
      return (
        <div className="settings-section form-grid">
          <h2>其他设置</h2>
          <label>
            <span>开机启动行为</span>
            <select
              value={settings.startupLaunchMode}
              onChange={(event) =>
                updateLaunchMode("startupLaunchMode", event.currentTarget.value)
              }
            >
              <option value="tray">启动到托盘</option>
              <option value="window">显示主窗口</option>
            </select>
          </label>
          <label>
            <span>双击启动行为</span>
            <select
              value={settings.manualLaunchMode}
              onChange={(event) =>
                updateLaunchMode("manualLaunchMode", event.currentTarget.value)
              }
            >
              <option value="tray">启动到托盘</option>
              <option value="window">显示主窗口</option>
            </select>
          </label>
          <label className="checkbox-row">
            <span>自动添加桌面快捷方式</span>
            <input type="checkbox" />
          </label>
          <label className="checkbox-row">
            <span>开机启动</span>
            <input type="checkbox" defaultChecked />
          </label>
        </div>
      );
    }

    return (
      <div className="settings-section info-section">
        <h2>信息</h2>
        <div className="info-content">
          <strong>TauriLaunch</strong>
          <span>版本 0.1.0</span>
          <span>Windows 软件启动器</span>
        </div>
      </div>
    );
  }

  return (
    <main className="settings-shell">
      <nav className="tabs" aria-label="设置页签" role="tablist">
        {settingsTabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={`tab${activeTab === tab.id ? " active" : ""}`}
            role="tab"
            aria-selected={activeTab === tab.id}
            onClick={() => setActiveTab(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </nav>

      <section className="settings-panel">{renderSettingsPanel()}</section>

      <footer className="dialog-actions">
        {saveError ? <span className="dialog-error">{saveError}</span> : null}
        <button type="button" className="primary" onClick={saveAndClose}>
          确定
        </button>
        <button type="button" onClick={closeSettings}>
          取消
        </button>
      </footer>
    </main>
  );
}

function AboutWindow() {
  return (
    <main className="about-shell">
      <div className="about-mark">TL</div>
      <h1>TauriLaunch</h1>
      <p>Windows 软件启动器</p>
      <p className="muted">版本 0.1.0</p>
    </main>
  );
}

export default App;
