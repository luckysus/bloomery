import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export function parseThemePreference(value: string | null): ThemePreference {
  if (!value) return "system";
  try {
    const parsed = JSON.parse(value) as { preference?: unknown };
    return parsed.preference === "light" || parsed.preference === "dark"
      ? parsed.preference
      : "system";
  } catch {
    return "system";
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

interface ThemeContextValue {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
}

const defaultTheme: ThemeContextValue = {
  preference: "system",
  resolvedTheme: "light",
  setPreference: () => undefined,
};

const ThemeContext = createContext<ThemeContextValue>(defaultTheme);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>("system");
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(readSystemTheme);
  const userChangedPreference = useRef(false);
  const resolvedTheme = resolveTheme(preference, systemTheme);

  useEffect(() => {
    let mounted = true;
    desktop.getSetting("ui.theme").then((value) => {
      if (mounted && !userChangedPreference.current) setPreferenceState(parseThemePreference(value));
    }).catch(() => undefined);
    return () => {
      mounted = false;
    };
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

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.style.colorScheme = resolvedTheme;
    if (isDesktopRuntime()) {
      const nativeThemeUpdate = desktop.setNativeTheme?.(resolvedTheme);
      if (nativeThemeUpdate) void nativeThemeUpdate.catch(() => undefined);
    }
  }, [resolvedTheme]);

  const value = useMemo<ThemeContextValue>(() => ({
    preference,
    resolvedTheme,
    setPreference: (next) => {
      userChangedPreference.current = true;
      setPreferenceState(next);
      void desktop.setSetting("ui.theme", JSON.stringify({ version: 1, preference: next }))
        .catch(() => undefined);
    },
  }), [preference, resolvedTheme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  return useContext(ThemeContext);
}
