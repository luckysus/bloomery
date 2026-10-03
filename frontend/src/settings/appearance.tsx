import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";

export type FontSizePreference = "small" | "medium" | "large";
export type DensityPreference = "compact" | "comfortable" | "spacious";
export type StartupPagePreference = "chat" | "knowledge" | "literature" | "data";
export type SidebarWidthPreference = "narrow" | "standard" | "wide";
export type AppearanceSaveState = "idle" | "saving" | "saved" | "error";

export interface AppearancePreferences {
  fontSize: FontSizePreference;
  density: DensityPreference;
  enableAnimations: boolean;
  showAgentPanel: boolean;
  restoreSession: boolean;
  saveDrafts: boolean;
  showToolDetails: boolean;
  confirmDangerous: boolean;
  startupPage: StartupPagePreference;
  autoUpdate: boolean;
  notifications: boolean;
  sidebarWidth: SidebarWidthPreference;
}

export const defaultAppearancePreferences: AppearancePreferences = {
  fontSize: "medium",
  density: "comfortable",
  enableAnimations: true,
  showAgentPanel: true,
  restoreSession: true,
  saveDrafts: true,
  showToolDetails: true,
  confirmDangerous: true,
  startupPage: "chat",
  autoUpdate: true,
  notifications: true,
  sidebarWidth: "standard",
};

function normalizePreferences(raw: unknown): AppearancePreferences {
  if (!raw || typeof raw !== "object") return defaultAppearancePreferences;
  const value = raw as Record<string, unknown>;
  const oldDensity = value.density === "standard" ? "comfortable" : value.density;
  return {
    ...defaultAppearancePreferences,
    ...value,
    fontSize: value.fontSize === "small" || value.fontSize === "large" ? value.fontSize : "medium",
    density: oldDensity === "compact" || oldDensity === "spacious" ? oldDensity : "comfortable",
    enableAnimations: typeof value.enableAnimations === "boolean" ? value.enableAnimations : value.reduceMotion === true ? false : true,
    showAgentPanel: typeof value.showAgentPanel === "boolean" ? value.showAgentPanel : typeof value.inspector === "boolean" ? value.inspector : true,
    restoreSession: typeof value.restoreSession === "boolean" ? value.restoreSession : true,
    saveDrafts: typeof value.saveDrafts === "boolean" ? value.saveDrafts : true,
    showToolDetails: typeof value.showToolDetails === "boolean" ? value.showToolDetails : true,
    confirmDangerous: typeof value.confirmDangerous === "boolean" ? value.confirmDangerous : true,
    startupPage: value.startupPage === "knowledge" || value.startupPage === "literature" || value.startupPage === "data" ? value.startupPage : "chat",
    autoUpdate: typeof value.autoUpdate === "boolean" ? value.autoUpdate : true,
    notifications: typeof value.notifications === "boolean" ? value.notifications : true,
    sidebarWidth: value.sidebarWidth === "narrow" || value.sidebarWidth === "wide" ? value.sidebarWidth : "standard",
  };
}

interface AppearanceContextValue {
  preferences: AppearancePreferences;
  updatePreferences: (next: Partial<AppearancePreferences>) => void;
  saveState: AppearanceSaveState;
  loaded: boolean;
  loadError: string | null;
  retryLoad: () => void;
  retrySave: () => void;
}

const defaultContext: AppearanceContextValue = {
  preferences: defaultAppearancePreferences,
  updatePreferences: () => undefined,
  saveState: "idle",
  // Isolated previews and tests use the safe defaults immediately. The real
  // provider still reports `false` while persisted preferences are loading.
  loaded: true,
  loadError: null,
  retryLoad: () => undefined,
  retrySave: () => undefined,
};

const AppearanceContext = createContext<AppearanceContextValue>(defaultContext);

export function AppearanceProvider({ children }: { children: ReactNode }) {
  const [preferences, setPreferences] = useState(defaultAppearancePreferences);
  const [saveState, setSaveState] = useState<AppearanceSaveState>("idle");
  const [loaded, setLoaded] = useState(false);
  const [loadNonce, setLoadNonce] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const pendingRef = useRef<AppearancePreferences>(defaultAppearancePreferences);
  const timerRef = useRef<number | null>(null);
  const requestRef = useRef(0);

  const persist = (next: AppearancePreferences) => {
    const request = ++requestRef.current;
    setSaveState("saving");
    if (!isDesktopRuntime()) {
      try {
        window.localStorage.setItem("suna.ui.preferences", JSON.stringify(next));
        if (request === requestRef.current) setSaveState("saved");
      } catch {
        if (request === requestRef.current) setSaveState("error");
      }
      return;
    }
    void desktop.setSetting("ui.preferences", JSON.stringify(next)).then(() => {
      if (request === requestRef.current) setSaveState("saved");
    }).catch(() => {
      if (request === requestRef.current) setSaveState("error");
    });
  };

  useEffect(() => {
    let mounted = true;
    const load = async () => {
      setLoadError(null);
      if (!isDesktopRuntime()) {
        try {
          const raw = window.localStorage.getItem("suna.ui.preferences");
        if (raw) {
          const next = normalizePreferences(JSON.parse(raw));
          pendingRef.current = next;
          setPreferences(next);
        }
      } catch {
          if (mounted) setLoadError("无法读取外观设置，请重试");
        } finally {
          if (mounted) setLoaded(true);
        }
        return;
      }
      try {
        for (let attempt = 0; attempt < 3 && mounted; attempt += 1) {
          try {
            const raw = await desktop.getSetting("ui.preferences");
            if (!mounted || !raw) return;
            try {
              const next = normalizePreferences(JSON.parse(raw));
              pendingRef.current = next;
              setPreferences(next);
            } catch {
              if (mounted) setLoadError("外观设置格式无效，已使用默认值");
            }
            return;
          } catch {
            if (attempt < 2) await new Promise((resolve) => window.setTimeout(resolve, 150));
            else if (mounted) setLoadError("无法读取外观设置，请重试");
          }
        }
      } finally {
        if (mounted) setLoaded(true);
      }
    };
    void load();
    return () => {
      mounted = false;
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    };
  }, [loadNonce]);

  useEffect(() => {
    const apply = (next: AppearancePreferences) => {
      pendingRef.current = next;
      setPreferences(next);
      setLoadError(null);
      setSaveState("saved");
    };
    const reset = () => apply(defaultAppearancePreferences);
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "ui.preferences" || !detail.value) return;
      try { apply(normalizePreferences(JSON.parse(detail.value))); } catch { setLoadError("外观设置格式无效，已使用默认值"); }
    };
    window.addEventListener("suna:settings-reset", reset);
    window.addEventListener("suna:setting-changed", changed);
    return () => {
      window.removeEventListener("suna:settings-reset", reset);
      window.removeEventListener("suna:setting-changed", changed);
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.fontSize = preferences.fontSize;
    root.dataset.density = preferences.density;
    root.dataset.motion = preferences.enableAnimations ? "enabled" : "reduced";
    root.dataset.sidebarWidth = preferences.sidebarWidth;
  }, [preferences]);

  const value = useMemo<AppearanceContextValue>(() => ({
    preferences,
    saveState,
    loaded,
    loadError,
    retryLoad: () => setLoadNonce((nonce) => nonce + 1),
    updatePreferences: (partial) => {
      const next = normalizePreferences({ ...pendingRef.current, ...partial });
      pendingRef.current = next;
      setPreferences(next);
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        timerRef.current = null;
        persist(next);
      }, 650);
    },
    retrySave: () => persist(pendingRef.current),
  }), [preferences, saveState, loaded, loadError]);

  return <AppearanceContext.Provider value={value}>{children}</AppearanceContext.Provider>;
}

export function useAppearanceSettings() {
  return useContext(AppearanceContext);
}
