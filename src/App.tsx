import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, FocusEvent, KeyboardEvent, MouseEvent } from "react";
import "./App.css";

type AppEntry = {
  id: string;
  name: string;
  path: string;
  launchArgs: string;
  workingDir: string;
  launches: number;
  accent: string;
  initials: string;
  searchText: string;
  source: string;
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
  watchedDirectories: string[];
};

type SettingsTab = "directories" | "icons" | "interface" | "other" | "info";

const defaultSettings: AppSettings = {
  windowRightOffset: 10,
  windowBottomOffset: 10,
  startupLaunchMode: "tray",
  manualLaunchMode: "window",
  watchedDirectories: [
    "C:\\Users\\fengqi\\Desktop\\App",
    "C:\\Users\\fengqi\\Desktop\\Game",
    "C:\\Users\\fengqi\\Desktop\\SingleExe",
  ],
};

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
  const [apps, setApps] = useState<AppEntry[]>([]);
  const [filteredApps, setFilteredApps] = useState<AppEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({
    left: 0,
    top: 0,
    visibility: "hidden",
  });
  const tooltipRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let canceled = false;

    invoke<AppEntry[]>("get_apps")
      .then((loadedApps) => {
        if (!canceled) {
          setApps(loadedApps);
          setFilteredApps(loadedApps);
          setError("");
        }
      })
      .catch((reason) => {
        if (!canceled) {
          setError(String(reason));
        }
      })
      .finally(() => {
        if (!canceled) {
          setLoading(false);
        }
      });

    const unlisten = listen<AppEntry[]>("apps-updated", (event) => {
      setApps(event.payload);
      setFilteredApps(event.payload);
      setLoading(false);
    });

    return () => {
      canceled = true;
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    let canceled = false;
    const keyword = query.trim();

    if (!keyword) {
      setFilteredApps(apps);
      return () => {
        canceled = true;
      };
    }

    const timer = window.setTimeout(() => {
      invoke<AppEntry[]>("search_apps", { query: keyword })
        .then((result) => {
          if (!canceled) {
            setFilteredApps(result);
            setError("");
          }
        })
        .catch((reason) => {
          if (!canceled) {
            setError(String(reason));
          }
        });
    }, 120);

    return () => {
      canceled = true;
      window.clearTimeout(timer);
    };
  }, [apps, query]);

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

  async function scanApps() {
    setLoading(true);
    try {
      const scanned = await invoke<AppEntry[]>("scan_apps");
      setApps(scanned);
      if (!query.trim()) {
        setFilteredApps(scanned);
      } else {
        const result = await invoke<AppEntry[]>("search_apps", { query });
        setFilteredApps(result);
      }
      setError("");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }

  function handleSearchKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" && filteredApps.length > 0) {
      event.preventDefault();
      void launchApp(filteredApps[0]);
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
            <span aria-hidden="true">⌕</span>
            <input
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder="搜索应用"
            />
          </label>
          <button type="button" className="toolbar-button" onClick={scanApps}>
            <span aria-hidden="true">↻</span>
            扫描
          </button>
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

      {loading ? (
        <section className="app-state">正在扫描应用...</section>
      ) : error ? (
        <section className="app-state error-state">{error}</section>
      ) : filteredApps.length === 0 ? (
        <section className="app-state">
          未找到应用，可在设置里添加监听目录后点击扫描。
        </section>
      ) : (
        <section className="app-grid" aria-label="应用列表">
          {filteredApps.map((app) => (
            <button
              key={app.id}
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
      )}

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
  const [directoryInput, setDirectoryInput] = useState("");
  const [selectedDirectory, setSelectedDirectory] = useState("");
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    let canceled = false;

    invoke<AppSettings>("get_settings")
      .then((loaded) => {
        if (!canceled) {
          setSettings({
            ...defaultSettings,
            ...loaded,
            watchedDirectories:
              loaded.watchedDirectories ?? defaultSettings.watchedDirectories,
          });
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

  function addDirectory() {
    const nextDirectory = directoryInput.trim();
    if (!nextDirectory) {
      return;
    }

    setSettings((current) => {
      if (
        current.watchedDirectories.some(
          (directory) =>
            directory.toLowerCase() === nextDirectory.toLowerCase(),
        )
      ) {
        return current;
      }

      return {
        ...current,
        watchedDirectories: [...current.watchedDirectories, nextDirectory],
      };
    });
    setDirectoryInput("");
    setSelectedDirectory(nextDirectory);
  }

  function removeDirectory() {
    if (!selectedDirectory) {
      return;
    }

    setSettings((current) => ({
      ...current,
      watchedDirectories: current.watchedDirectories.filter(
        (directory) => directory !== selectedDirectory,
      ),
    }));
    setSelectedDirectory("");
  }

  function renderSettingsPanel() {
    if (activeTab === "directories") {
      return (
        <div className="settings-section directory-section">
          <h2>目录监听</h2>
          <div className="directory-list" role="listbox" aria-label="监听目录">
            {settings.watchedDirectories.map((directory) => (
              <button
                key={directory}
                type="button"
                className={directory === selectedDirectory ? "selected" : ""}
                onClick={() => {
                  setSelectedDirectory(directory);
                  setDirectoryInput(directory);
                }}
              >
                {directory}
              </button>
            ))}
          </div>
          <div className="directory-actions">
            <input
              aria-label="目录路径"
              value={directoryInput}
              onChange={(event) => setDirectoryInput(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addDirectory();
                }
              }}
            />
            <button type="button">浏览</button>
            <button type="button" onClick={addDirectory}>
              添加
            </button>
            <button type="button" onClick={removeDirectory}>
              删除
            </button>
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
