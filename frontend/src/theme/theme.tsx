import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type ThemeSaveState = "idle" | "saving" | "saved" | "error";
const THEME_STORAGE_KEY = "suna.ui.theme";

export function parseThemePreference(value: string | null): ThemePreference {
  if (!value) return "light";
  try {
    const parsed = JSON.parse(value) as { preference?: unknown };
    return parsed.preference === "light" || parsed.preference === "dark" || parsed.preference === "system"
      ? parsed.preference
      : "light";
  } catch {
    return "light";
  }
}

export function resolveTheme(
  preference: ThemePreference,
  systemTheme: ResolvedTheme = "light",
): ResolvedTheme {
  return preference === "system" ? systemTheme : preference;
}

function readSystemTheme(): ResolvedTheme {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}


function readStoredTheme(): ThemePreference {
  if (typeof window === "undefined") return "light";
  try {
    return parseThemePreference(window.localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "light";
  }
}

interface ThemeContextValue {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
  saveState: ThemeSaveState;
  loadError: string | null;
  retryLoad: () => void;
  retrySave: () => void;
}

const defaultTheme: ThemeContextValue = {
  preference: "light",
  resolvedTheme: "light",
  setPreference: () => undefined,
  saveState: "idle",
  loadError: null,
  retryLoad: () => undefined,
  retrySave: () => undefined,
};

const ThemeContext = createContext<ThemeContextValue>(defaultTheme);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>("light");
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(readSystemTheme);
  const [saveState, setSaveState] = useState<ThemeSaveState>("idle");
  const [loadNonce, setLoadNonce] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const pendingRef = useRef<ThemePreference>("light");
  const requestRef = useRef(0);
  const userChangedPreference = useRef(false);
  const resolvedTheme = resolveTheme(preference, systemTheme);

  useEffect(() => {
    let mounted = true;
    setLoadError(null);
    const desktopRuntime = isDesktopRuntime();
    // Ask the bridge first even in the browser preview. The Tauri bridge
    // rejects there, and the catch path then falls back to localStorage. This
    // keeps the desktop source authoritative while keeping previews usable.
    desktop.getSetting("ui.theme").then((value) => {
      if (mounted && !userChangedPreference.current) {
        // A desktop setting is authoritative. Only the browser preview uses
        // localStorage as a fallback when no bridge value exists; otherwise a
        // stale preview value could silently override a cleared desktop
        // preference.
        const next = value ? parseThemePreference(value) : desktopRuntime ? "light" : readStoredTheme();
        pendingRef.current = next;
        setPreferenceState(next);
      }
    }).catch(() => {
      if (mounted && !userChangedPreference.current) {
        const next = desktopRuntime ? "light" : readStoredTheme();
        pendingRef.current = next;
        setPreferenceState(next);
        if (desktopRuntime) setLoadError("无法读取主题设置，请重试");
      }
    });
    return () => {
      mounted = false;
    };
  }, [loadNonce]);

  useEffect(() => {
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "ui.theme" || !detail.value) return;
      userChangedPreference.current = true;
      const next = parseThemePreference(detail.value);
      pendingRef.current = next;
      setPreferenceState(next);
      setLoadError(null);
      setSaveState("saved");
    };
    window.addEventListener("suna:setting-changed", changed);
    return () => window.removeEventListener("suna:setting-changed", changed);
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemTheme(media.matches ? "dark" : "light");
    update();
    if (preference !== "system") return;

    media.addEventListener?.("change", update);
    if (!media.addEventListener) media.addListener?.(update);
    return () => {
      media.removeEventListener?.("change", update);
      if (!media.removeEventListener) media.removeListener?.(update);
    };
  }, [preference]);

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.style.colorScheme = resolvedTheme;
    if (isDesktopRuntime()) {
      const nativeThemeUpdate = desktop.setNativeTheme?.(resolvedTheme);
      if (nativeThemeUpdate) void nativeThemeUpdate.catch(() => undefined);
    }
  }, [resolvedTheme]);

  const persist = (next: ThemePreference) => {
    const request = ++requestRef.current;
    pendingRef.current = next;
    setSaveState("saving");
    let localSaved = true;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, JSON.stringify({ version: 1, preference: next }));
    } catch {
      localSaved = false;
      if (request === requestRef.current) setSaveState("error");
    }
    // Persist through the desktop bridge as the authoritative store. In a
    // browser preview this rejects harmlessly after localStorage has already
    // been updated, and the catch below keeps the preview in a saved state.
    if (!isDesktopRuntime() && !localSaved) return;
    if (!isDesktopRuntime()) {
      if (request === requestRef.current) setSaveState("saved");
      return;
    }
    void desktop.setSetting("ui.theme", JSON.stringify({ version: 1, preference: next }))
      .then(() => { if (request === requestRef.current) setSaveState("saved"); })
      .catch(() => { if (request === requestRef.current) setSaveState(isDesktopRuntime() ? "error" : "saved"); });
  };

  const value = useMemo<ThemeContextValue>(() => ({
    preference,
    resolvedTheme,
    saveState,
    loadError,
    retryLoad: () => setLoadNonce((nonce) => nonce + 1),
    retrySave: () => persist(pendingRef.current),
    setPreference: (next) => {
      userChangedPreference.current = true;
      pendingRef.current = next;
      setPreferenceState(next);
      persist(next);
    },
  }), [preference, resolvedTheme, saveState, loadError]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  return useContext(ThemeContext);
}
