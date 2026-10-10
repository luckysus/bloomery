import { useEffect, useLayoutEffect, type ReactNode } from "react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import { applyExternalTheme, useSettingsStore } from "../stores/settingsStore";
import { resolveTheme, type ResolvedTheme, type ThemePreference, type ThemeSaveState } from "./themeModel";

export { parseThemePreference, resolveTheme } from "./themeModel";
export type { ResolvedTheme, ThemePreference, ThemeSaveState } from "./themeModel";

/**
 * 主题 Provider。
 *
 * 第 77 章要求状态由 Zustand 承载，因此这里不再持有状态：主题的唯一事实来源是
 * `stores/settingsStore`。Provider 只负责三件事——首次加载、同步外部设置事件、
 * 把解析后的主题写到 document 上。
 */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const loadTheme = useSettingsStore((state) => state.loadTheme);
  const syncSystemTheme = useSettingsStore((state) => state.syncSystemTheme);
  const preference = useSettingsStore((state) => state.themePreference);
  const systemTheme = useSettingsStore((state) => state.systemTheme);
  const resolvedTheme = resolveTheme(preference, systemTheme);

  useEffect(() => {
    void loadTheme();
  }, [loadTheme]);

  useEffect(() => {
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "ui.theme" || !detail.value) return;
      applyExternalTheme(detail.value);
    };
    window.addEventListener("suna:setting-changed", changed);
    return () => window.removeEventListener("suna:setting-changed", changed);
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => syncSystemTheme();
    update();
    if (preference !== "system") return;

    media.addEventListener?.("change", update);
    if (!media.addEventListener) media.addListener?.(update);
    return () => {
      media.removeEventListener?.("change", update);
      if (!media.removeEventListener) media.removeListener?.(update);
    };
  }, [preference, syncSystemTheme]);

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.style.colorScheme = resolvedTheme;
    if (isDesktopRuntime()) {
      const nativeThemeUpdate = desktop.setNativeTheme?.(resolvedTheme);
      if (nativeThemeUpdate) void nativeThemeUpdate.catch(() => undefined);
    }
  }, [resolvedTheme]);

  return <>{children}</>;
}

export function useTheme() {
  const preference = useSettingsStore((state) => state.themePreference);
  const systemTheme = useSettingsStore((state) => state.systemTheme);
  const setPreference = useSettingsStore((state) => state.setThemePreference);
  const saveState = useSettingsStore((state) => state.themeSaveState);
  const loadError = useSettingsStore((state) => state.themeLoadError);
  const retryLoad = useSettingsStore((state) => state.retryThemeLoad);
  const retrySave = useSettingsStore((state) => state.retryThemeSave);

  return {
    preference,
    resolvedTheme: resolveTheme(preference, systemTheme),
    setPreference,
    saveState,
    loadError,
    retryLoad,
    retrySave,
  };
}
