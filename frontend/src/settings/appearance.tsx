import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { desktop } from "../bridge/desktop";

export type FontSizePreference = "small" | "medium" | "large";
export type DensityPreference = "compact" | "comfortable" | "spacious";
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
  };
}

interface AppearanceContextValue {
  preferences: AppearancePreferences;
  updatePreferences: (next: Partial<AppearancePreferences>) => void;
  saveState: AppearanceSaveState;
  retrySave: () => void;
}

const defaultContext: AppearanceContextValue = {
  preferences: defaultAppearancePreferences,
  updatePreferences: () => undefined,
  saveState: "idle",
  retrySave: () => undefined,
};

const AppearanceContext = createContext<AppearanceContextValue>(defaultContext);

export function AppearanceProvider({ children }: { children: ReactNode }) {
  const [preferences, setPreferences] = useState(defaultAppearancePreferences);
  const [saveState, setSaveState] = useState<AppearanceSaveState>("idle");
  const pendingRef = useRef<AppearancePreferences>(defaultAppearancePreferences);
  const timerRef = useRef<number | null>(null);
  const requestRef = useRef(0);

  const persist = (next: AppearancePreferences) => {
    const request = ++requestRef.current;
    setSaveState("saving");
    void desktop.setSetting("ui.preferences", JSON.stringify(next)).then(() => {
      if (request === requestRef.current) setSaveState("saved");
    }).catch(() => {
      if (request === requestRef.current) setSaveState("error");
    });
  };

  useEffect(() => {
    let mounted = true;
    const load = async () => {
      for (let attempt = 0; attempt < 3 && mounted; attempt += 1) {
        try {
          const raw = await desktop.getSetting("ui.preferences");
          if (!mounted || !raw) return;
          try {
            const next = normalizePreferences(JSON.parse(raw));
            pendingRef.current = next;
            setPreferences(next);
          } catch {
            // Keep the safe defaults when older or invalid settings are present.
          }
          return;
        } catch {
          if (attempt < 2) await new Promise((resolve) => window.setTimeout(resolve, 150));
        }
      }
    };
    void load();
    return () => {
      mounted = false;
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.fontSize = preferences.fontSize;
    root.dataset.density = preferences.density;
    root.dataset.motion = preferences.enableAnimations ? "enabled" : "reduced";
  }, [preferences]);

  const value = useMemo<AppearanceContextValue>(() => ({
    preferences,
    saveState,
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
  }), [preferences, saveState]);

  return <AppearanceContext.Provider value={value}>{children}</AppearanceContext.Provider>;
}

export function useAppearanceSettings() {
  return useContext(AppearanceContext);
}
