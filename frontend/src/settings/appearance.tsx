import { useEffect, type ReactNode } from "react";
import {
  applyExternalAppearance,
  ensureAppearanceLoaded,
  useSettingsStore,
} from "../stores/settingsStore";
import { defaultAppearancePreferences } from "./appearanceModel";

export { defaultAppearancePreferences, normalizePreferences } from "./appearanceModel";
export type {
  AppearancePreferences,
  AppearanceSaveState,
  DensityPreference,
  FontSizePreference,
  SidebarWidthPreference,
  StartupPagePreference,
} from "./appearanceModel";

/**
 * 外观设置 Provider。
 *
 * 第 77 章要求状态由 Zustand 承载，因此这里不再持有状态：外观偏好的唯一事实来源
 * 是 `stores/settingsStore`。Provider 只负责触发首次加载、同步外部设置事件，并把
 * 外观相关的 data-* 属性写到 document 上。
 */
export function AppearanceProvider({ children }: { children: ReactNode }) {
  const preferences = useSettingsStore((state) => state.preferences);

  useEffect(() => {
    ensureAppearanceLoaded();
  }, []);

  useEffect(() => {
    const reset = () => {
      useSettingsStore.setState({
        preferences: defaultAppearancePreferences,
        appearanceSaveState: "saved",
        appearanceLoadError: null,
        appearanceLoaded: true,
      });
    };
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "ui.preferences" || !detail.value) return;
      applyExternalAppearance(detail.value);
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

  return <>{children}</>;
}

export function useAppearanceSettings() {
  const preferences = useSettingsStore((state) => state.preferences);
  const updatePreferences = useSettingsStore((state) => state.updatePreferences);
  const saveState = useSettingsStore((state) => state.appearanceSaveState);
  const loaded = useSettingsStore((state) => state.appearanceLoaded);
  const loadError = useSettingsStore((state) => state.appearanceLoadError);
  const retryLoad = useSettingsStore((state) => state.retryAppearanceLoad);
  const retrySave = useSettingsStore((state) => state.retryAppearanceSave);

  // 迁移前「没有 Provider 时立即使用安全默认值」的语义，改由 hook 触发一次懒加载。
  useEffect(() => {
    ensureAppearanceLoaded();
  }, []);

  return { preferences, updatePreferences, saveState, loaded, loadError, retryLoad, retrySave };
}
