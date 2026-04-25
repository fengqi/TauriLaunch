import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type AppEntry = {
  id: string;
  name: string;
  path: string;
  launches: number;
  accent: string;
  initials: string;
};

type AppSettings = {
  windowRightOffset: number;
  windowBottomOffset: number;
  startupLaunchMode: "tray" | "window";
  manualLaunchMode: "tray" | "window";
};

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
    launches: 35,
    accent: "#245b9f",
    initials: "St",
  },
  {
    id: "wechat",
    name: "微信",
    path: "C:\\Program Files\\Tencent\\WeChat\\WeChat.exe",
    launches: 18,
    accent: "#28c76f",
    initials: "微",
  },
  {
    id: "notepad2",
    name: "Notepad2",
    path: "C:\\Tools\\Notepad2\\Notepad2.exe",
    launches: 9,
    accent: "#8bb8c8",
    initials: "N2",
  },
  {
    id: "uu",
    name: "UU加速器",
    path: "C:\\Program Files\\Netease\\UU\\uu.exe",
    launches: 4,
    accent: "#06a6d8",
    initials: "UU",
  },
  {
    id: "kook",
    name: "KOOK",
    path: "C:\\Users\\fengqi\\AppData\\Local\\KOOK\\KOOK.exe",
    launches: 12,
    accent: "#64d923",
    initials: "K",
  },
  {
    id: "vscode",
    name: "VS Code",
    path: "C:\\Users\\fengqi\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
    launches: 41,
    accent: "#2f80ed",
    initials: "VS",
  },
  {
    id: "everything",
    name: "Everything",
    path: "C:\\Program Files\\Everything\\Everything.exe",
    launches: 28,
    accent: "#ff8a00",
    initials: "Ev",
  },
  {
    id: "telegram",
    name: "Telegram",
    path: "C:\\Users\\fengqi\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe",
    launches: 22,
    accent: "#2aabee",
    initials: "T",
  },
  {
    id: "obs",
    name: "OBS Studio",
    path: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe",
    launches: 7,
    accent: "#303238",
    initials: "OBS",
  },
  {
    id: "potplayer",
    name: "PotPlayer",
    path: "C:\\Program Files\\DAUM\\PotPlayer\\PotPlayerMini64.exe",
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

  async function closeMainWindow() {
    await invoke("dismiss_main_window");
  }

  async function launchApp(app: AppEntry) {
    await invoke("dismiss_after_launch", { appName: app.name });
  }

  function handleSearchKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" && apps.length > 0) {
      event.preventDefault();
      launchApp(apps[0]);
    }
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
            <span aria-hidden="true">⌕</span>
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
        {apps.map((app) => (
          <button
            key={app.id}
            type="button"
            className="app-tile"
            onDoubleClick={() => launchApp(app)}
            title={`启动次数: ${app.launches}\n路径: ${app.path}`}
          >
            <span className="app-icon" style={{ background: app.accent }}>
              {app.initials}
            </span>
            <span className="app-name">{app.name}</span>
            <span className="app-tooltip">
              <span>启动次数: {app.launches}</span>
              <span>路径: {app.path}</span>
            </span>
          </button>
        ))}
      </section>
    </main>
  );
}

function SettingsWindow() {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
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
      await invoke("save_settings", { settings });
      await invoke("close_settings_window");
    } catch (error) {
      setSaveError(String(error));
    }
  }

  async function closeSettings() {
    await invoke("close_settings_window");
  }

  function updateSetting(name: keyof AppSettings, value: string) {
    setSettings((current) => ({
      ...current,
      [name]:
        name === "windowRightOffset" || name === "windowBottomOffset"
          ? Math.max(0, Number.parseInt(value, 10) || 0)
          : value,
    }));
  }

  return (
    <main className="settings-shell">
      <header className="dialog-title">
        <span className="gear" aria-hidden="true">
          ⚙
        </span>
        <span>设置</span>
      </header>

      <nav className="tabs" aria-label="设置页签">
        <button className="tab active">目录监听</button>
        <button className="tab">图标设置</button>
        <button className="tab">界面设置</button>
        <button className="tab">其他设置</button>
        <button className="tab">信息</button>
      </nav>

      <section className="settings-panel">
        <div className="settings-section directory-section">
          <h2>目录监听</h2>
          <div className="directory-list">
            {directories.map((directory) => (
              <div key={directory}>{directory}</div>
            ))}
          </div>
          <div className="directory-actions">
            <input aria-label="目录路径" />
            <button>浏览</button>
            <button>添加</button>
            <button>删除</button>
          </div>
        </div>

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

        <div className="settings-section form-grid">
          <h2>界面设置</h2>
          <label>
            <span>窗口位置与屏幕右边距离</span>
            <input
              type="number"
              min={0}
              value={settings.windowRightOffset}
              onChange={(event) =>
                updateSetting("windowRightOffset", event.currentTarget.value)
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
                updateSetting("windowBottomOffset", event.currentTarget.value)
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

        <div className="settings-section form-grid">
          <h2>其他设置</h2>
          <label>
            <span>开机启动行为</span>
            <select
              value={settings.startupLaunchMode}
              onChange={(event) =>
                updateSetting("startupLaunchMode", event.currentTarget.value)
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
                updateSetting("manualLaunchMode", event.currentTarget.value)
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
      </section>

      <footer className="dialog-actions">
        {saveError ? <span className="dialog-error">{saveError}</span> : null}
        <button className="primary" onClick={saveAndClose}>
          确定
        </button>
        <button onClick={closeSettings}>取消</button>
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
