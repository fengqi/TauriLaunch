import { listen } from "@tauri-apps/api/event";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Component, useEffect, useLayoutEffect, useRef, useState } from "react";
import type {
  CSSProperties,
  FocusEvent,
  KeyboardEvent,
  MouseEvent,
  ReactNode,
} from "react";
import "./App.css";

type AppEntry = {
  id: string;
  name: string;
  customName: string;
  path: string;
  launchArgs: string;
  workingDir: string;
  launches: number;
  accent: string;
  initials: string;
  searchText: string;
  iconPath: string;
  source: string;
  hidden: boolean;
  lastError: string;
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

type AppContextMenuState = {
  app: AppEntry;
  left: number;
  top: number;
};

type LaunchMode = "tray" | "window";

type AppSettings = {
  windowRightOffset: number;
  windowBottomOffset: number;
  startupLaunchMode: LaunchMode;
  manualLaunchMode: LaunchMode;
  iconSize: number;
  autostartEnabled: boolean;
  physicalDeleteEnabled: boolean;
  showHiddenApps: boolean;
  watchedDirectories: string[];
};

type SettingsTab = "directories" | "icons" | "interface" | "other" | "info";

const defaultSettings: AppSettings = {
  windowRightOffset: 10,
  windowBottomOffset: 10,
  startupLaunchMode: "tray",
  manualLaunchMode: "window",
  iconSize: 38,
  autostartEnabled: false,
  physicalDeleteEnabled: false,
  showHiddenApps: false,
  watchedDirectories: [],
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
    return (
      <AppErrorBoundary>
        <SettingsWindow />
      </AppErrorBoundary>
    );
  }

  if (view === "about") {
    return (
      <AppErrorBoundary>
        <AboutWindow />
      </AppErrorBoundary>
    );
  }

  return (
    <AppErrorBoundary>
      <LauncherWindow />
    </AppErrorBoundary>
  );
}

type AppErrorBoundaryProps = {
  children: ReactNode;
};

type AppErrorBoundaryState = {
  error: string;
};

class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { error: "" };

  static getDerivedStateFromError(error: unknown): AppErrorBoundaryState {
    return { error: String(error) };
  }

  render() {
    if (this.state.error) {
      return <main className="app-state error-state">{this.state.error}</main>;
    }

    return this.props.children;
  }
}

function useDisableNativeContextMenu() {
  useEffect(() => {
    const handleContextMenu = (event: globalThis.MouseEvent) => {
      event.preventDefault();
    };

    window.addEventListener("contextmenu", handleContextMenu);
    return () => window.removeEventListener("contextmenu", handleContextMenu);
  }, []);
}

function LauncherWindow() {
  const [query, setQuery] = useState("");
  const [apps, setApps] = useState<AppEntry[]>([]);
  const [filteredApps, setFilteredApps] = useState<AppEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [iconSize, setIconSize] = useState(defaultSettings.iconSize);
  const [scanning, setScanning] = useState(false);
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const [contextMenu, setContextMenu] = useState<AppContextMenuState | null>(
    null,
  );
  const [editingAppId, setEditingAppId] = useState("");
  const [editingName, setEditingName] = useState("");
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({
    left: 0,
    top: 0,
    visibility: "hidden",
  });
  const appsRef = useRef<AppEntry[]>([]);
  const queryRef = useRef("");
  const searchInputRef = useRef<HTMLInputElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);

  function focusSearchInput() {
    window.setTimeout(() => searchInputRef.current?.focus(), 0);
  }

  function clearSearch() {
    setQuery("");
    setFilteredApps(appsRef.current);
    setTooltip(null);
  }

  useEffect(() => {
    appsRef.current = apps;
  }, [apps]);

  useEffect(() => {
    queryRef.current = query;
  }, [query]);

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
      setLoading(false);
      const keyword = queryRef.current.trim();
      if (!keyword) {
        setFilteredApps(event.payload);
        return;
      }

      void invoke<AppEntry[]>("search_apps", { query: keyword })
        .then((result) => {
          if (queryRef.current.trim() === keyword) {
            setFilteredApps(result);
          }
        })
        .catch((reason) => setError(String(reason)));
    });
    const unlistenSettings = listen<AppSettings>("settings-updated", (event) => {
      setIconSize(event.payload.iconSize || defaultSettings.iconSize);
      void invoke<AppEntry[]>("get_apps")
        .then((updatedApps) => applyUpdatedApps(updatedApps))
        .catch((reason) => setError(String(reason)));
    });
    const unlistenDismissed = listen("main-window-dismissed", () => {
      clearSearch();
    });
    const unlistenScanFailed = listen<string>("scan-failed", (event) => {
      setError(event.payload);
      setScanning(false);
    });
    const unlistenScanState = listen<boolean>("scan-state-changed", (event) => {
      setScanning(Boolean(event.payload));
    });
    const unlistenFocusSearch = listen("focus-search", () => {
      focusSearchInput();
    });

    focusSearchInput();

    return () => {
      canceled = true;
      void unlisten.then((dispose) => dispose());
      void unlistenSettings.then((dispose) => dispose());
      void unlistenDismissed.then((dispose) => dispose());
      void unlistenScanFailed.then((dispose) => dispose());
      void unlistenScanState.then((dispose) => dispose());
      void unlistenFocusSearch.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    let canceled = false;

    invoke<AppSettings>("get_settings")
      .then((settings) => {
        if (!canceled) {
          setIconSize(settings.iconSize || defaultSettings.iconSize);
        }
      })
      .catch(() => undefined);

    return () => {
      canceled = true;
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

  useEffect(() => {
    if (editingAppId) {
      nameInputRef.current?.focus();
      nameInputRef.current?.select();
    }
  }, [editingAppId]);

  async function closeMainWindow() {
    clearSearch();
    await invoke("dismiss_main_window");
  }

  async function launchApp(app: AppEntry) {
    if (editingAppId) {
      return;
    }

    try {
      const updatedApps = await invoke<AppEntry[]>("launch_app", {
        appId: app.id,
      });
      clearSearch();
      setApps(updatedApps);
      setFilteredApps(updatedApps);
      setTooltip(null);
      setError("");
    } catch (reason) {
      setError(String(reason));
      const updatedApps = await invoke<AppEntry[]>("get_apps");
      setApps(updatedApps);
      if (!query.trim()) {
        setFilteredApps(updatedApps);
      }
    }
  }

  async function applyUpdatedApps(updatedApps: AppEntry[]) {
    setApps(updatedApps);
    if (!query.trim()) {
      setFilteredApps(updatedApps);
      return;
    }

    const result = await invoke<AppEntry[]>("search_apps", { query });
    setFilteredApps(result);
  }

  async function hideApp(app: AppEntry) {
    try {
      const updatedApps = await invoke<AppEntry[]>("hide_app", {
        appId: app.id,
      });
      await applyUpdatedApps(updatedApps);
      setTooltip(null);
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function pinApp(app: AppEntry) {
    try {
      const updatedApps = await invoke<AppEntry[]>("pin_app", {
        appId: app.id,
      });
      await applyUpdatedApps(updatedApps);
      setTooltip(null);
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function resetAppPosition(app: AppEntry) {
    try {
      const updatedApps = await invoke<AppEntry[]>("reset_app_position", {
        appId: app.id,
      });
      await applyUpdatedApps(updatedApps);
      setTooltip(null);
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function openAppDirectory(app: AppEntry) {
    try {
      await invoke("open_app_directory", { appId: app.id });
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  function startRename(app: AppEntry) {
    setTooltip(null);
    setEditingAppId(app.id);
    setEditingName(app.name);
  }

  function cancelRename() {
    setEditingAppId("");
    setEditingName("");
  }

  async function saveRename(app: AppEntry) {
    const name = editingName.trim();
    if (!name || name === app.name) {
      cancelRename();
      return;
    }

    try {
      const updatedApps = await invoke<AppEntry[]>("rename_app", {
        appId: app.id,
        name,
      });
      await applyUpdatedApps(updatedApps);
      cancelRename();
      setTooltip(null);
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function scanApps() {
    try {
      setScanning(true);
      await invoke("scan_apps");
      setError("");
    } catch (reason) {
      setScanning(false);
      setError(String(reason));
    }
  }

  async function addApp() {
    try {
      await invoke("set_main_dialog_open", { open: true });
      const selected = await open({
        directory: false,
        multiple: false,
        filters: [
          {
            name: "应用",
            extensions: ["exe", "lnk"],
          },
        ],
      });

      if (typeof selected !== "string") {
        return;
      }

      const updatedApps = await invoke<AppEntry[]>("add_app", {
        path: selected,
      });
      setApps(updatedApps);
      if (!query.trim()) {
        setFilteredApps(updatedApps);
      } else {
        const result = await invoke<AppEntry[]>("search_apps", { query });
        setFilteredApps(result);
      }
      setTooltip(null);
      setError("");
    } catch (reason) {
      setError(String(reason));
    } finally {
      await invoke("set_main_dialog_open", { open: false }).catch(() => undefined);
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
    event: MouseEvent<HTMLDivElement> | FocusEvent<HTMLDivElement>,
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

  function showAppContextMenu(
    app: AppEntry,
    event: MouseEvent<HTMLDivElement>,
  ) {
    event.preventDefault();
    event.stopPropagation();
    setTooltip(null);
    setContextMenu({
      app,
      left: Math.min(event.clientX, window.innerWidth - 160),
      top: Math.min(event.clientY, window.innerHeight - 170),
    });
  }

  function closeContextMenu() {
    setContextMenu(null);
  }

  return (
    <main
      className="launcher-shell"
      onClick={closeContextMenu}
      onContextMenu={(event) => {
        event.preventDefault();
        closeContextMenu();
      }}
    >
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
          <div className="search-box">
            <span aria-hidden="true">⌕</span>
            <input
              aria-label="搜索应用"
              ref={searchInputRef}
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder="搜索应用"
            />
            {query ? (
              <button
                type="button"
                className="search-clear"
                aria-label="清空搜索"
                onClick={clearSearch}
              />
            ) : null}
          </div>
          <button
            type="button"
            className="toolbar-button"
            onClick={scanApps}
            disabled={scanning}
          >
            <span
              aria-hidden="true"
              className={scanning ? "scan-spinner" : undefined}
            >
              ↻
            </span>
            {scanning ? "扫描中" : "扫描"}
          </button>
          <button type="button" className="toolbar-button" onClick={addApp}>
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
        <section className="app-state error-state">扫描失败：{error}</section>
      ) : filteredApps.length === 0 ? (
        <section className="app-state">
          {query.trim()
            ? "没有匹配的应用。"
            : "未找到应用。请在设置里添加监听目录，或确认目录中存在 .exe / .lnk 文件。"}
        </section>
      ) : (
        <section
          className="app-grid"
          aria-label="应用列表"
          style={{ "--app-icon-size": `${iconSize}px` } as CSSProperties}
        >
          {filteredApps.map((app) => (
            <div
              key={app.id}
              className={`app-tile${app.hidden ? " hidden-app" : ""}`}
              role="button"
              tabIndex={0}
              onClick={() => void launchApp(app)}
              onKeyDown={(event) => {
                if (
                  !editingAppId &&
                  (event.key === "Enter" || event.key === " ")
                ) {
                  event.preventDefault();
                  void launchApp(app);
                }
              }}
              onContextMenu={(event) => showAppContextMenu(app, event)}
              onMouseEnter={(event) => showAppTooltip(app, event)}
              onMouseLeave={hideAppTooltip}
              onFocus={(event) => showAppTooltip(app, event)}
              onBlur={hideAppTooltip}
            >
              {app.iconPath ? (
                <span className="app-icon image-icon">
                  <img src={convertFileSrc(app.iconPath)} alt="" />
                </span>
              ) : (
                <span className="app-icon" style={{ background: app.accent }}>
                  {app.initials}
                </span>
              )}
              {editingAppId === app.id ? (
                <input
                  ref={nameInputRef}
                  className="app-name-input"
                  value={editingName}
                  onClick={(event) => event.stopPropagation()}
                  onMouseDown={(event) => event.stopPropagation()}
                  onChange={(event) => setEditingName(event.currentTarget.value)}
                  onBlur={cancelRename}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      event.stopPropagation();
                      void saveRename(app);
                    }
                    if (event.key === "Escape") {
                      event.preventDefault();
                      event.stopPropagation();
                      cancelRename();
                    }
                  }}
                />
              ) : (
                <span className="app-name">{app.name}</span>
              )}
            </div>
          ))}
        </section>
      )}

      {contextMenu ? (
        <div
          className="app-context-menu"
          style={{ left: contextMenu.left, top: contextMenu.top }}
          onClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.preventDefault()}
        >
          <button
            type="button"
            onClick={() => {
              closeContextMenu();
              void pinApp(contextMenu.app);
            }}
          >
            置顶
          </button>
          <button
            type="button"
            onClick={() => {
              closeContextMenu();
              void resetAppPosition(contextMenu.app);
            }}
          >
            重置位置
          </button>
          <button
            type="button"
            onClick={() => {
              const app = contextMenu.app;
              closeContextMenu();
              startRename(app);
            }}
          >
            修改名称
          </button>
          <button
            type="button"
            onClick={() => {
              closeContextMenu();
              void openAppDirectory(contextMenu.app);
            }}
          >
            打开所在目录
          </button>
          <button
            type="button"
            className="danger"
            onClick={() => {
              closeContextMenu();
              void hideApp(contextMenu.app);
            }}
          >
            删除
          </button>
        </div>
      ) : null}

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
          {tooltip.app.workingDir ? (
            <span>工作目录: {tooltip.app.workingDir}</span>
          ) : null}
          {tooltip.app.lastError ? (
            <span className="tooltip-error">错误信息: {tooltip.app.lastError}</span>
          ) : null}
        </div>
      ) : null}
    </main>
  );
}

function SettingsWindow() {
  useDisableNativeContextMenu();
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
            iconSize: loaded.iconSize || defaultSettings.iconSize,
            autostartEnabled: Boolean(loaded.autostartEnabled),
            physicalDeleteEnabled: Boolean(loaded.physicalDeleteEnabled),
            showHiddenApps: Boolean(loaded.showHiddenApps),
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

  function updateIconSize(value: string) {
    const iconSize = Number.parseInt(value, 10);
    if (![32, 38, 48].includes(iconSize)) {
      return;
    }

    setSettings((current) => ({
      ...current,
      iconSize,
    }));
  }

  function updateBooleanSetting(
    name: "autostartEnabled" | "physicalDeleteEnabled" | "showHiddenApps",
    checked: boolean,
  ) {
    setSettings((current) => ({
      ...defaultSettings,
      ...current,
      [name]: checked,
    }));
  }

  async function chooseDirectory() {
    const selected = await open({
      directory: true,
      multiple: false,
    });

    if (typeof selected !== "string") {
      return;
    }

    setDirectoryInput(selected);
    setSelectedDirectory(selected);
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
            <button type="button" onClick={() => void chooseDirectory()}>
              浏览
            </button>
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
            <select
              value={String(settings.iconSize)}
              onChange={(event) => updateIconSize(event.currentTarget.value)}
            >
              <option value="32">32x32</option>
              <option value="38">38x38</option>
              <option value="48">48x48</option>
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
            <span>物理删除应用记录</span>
            <input
              type="checkbox"
              checked={settings.physicalDeleteEnabled}
              onChange={(event) =>
                updateBooleanSetting(
                  "physicalDeleteEnabled",
                  event.currentTarget.checked,
                )
              }
            />
          </label>
          <label className="checkbox-row">
            <span>显示隐藏的应用</span>
            <input
              type="checkbox"
              checked={settings.showHiddenApps}
              onChange={(event) =>
                updateBooleanSetting("showHiddenApps", event.currentTarget.checked)
              }
            />
          </label>
          <label className="checkbox-row">
            <span>开机启动</span>
            <input
              type="checkbox"
              checked={settings.autostartEnabled}
              onChange={(event) =>
                updateBooleanSetting("autostartEnabled", event.currentTarget.checked)
              }
            />
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
  useDisableNativeContextMenu();

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
