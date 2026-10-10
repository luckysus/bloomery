/**
 * settingsStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载外观偏好（appearance）与主题（theme）两类设置的**唯一事实来源**。
 * 迁移前它们分别放在 `settings/appearance.tsx` 与 `theme/theme.tsx` 的 React
 * Context 中；现在 Provider 只保留「触发加载 + DOM 副作用」，状态与持久化都在本
 * store 内，因此消费方的 hook 签名保持不变。
 *
 * 持久化细节见 `stores/settingsPersistence.ts`。
 */
import { create } from "zustand";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import {
  defaultAppearancePreferences,
  normalizePreferences,
  type AppearancePreferences,
  type AppearanceSaveState,
} from "../settings/appearanceModel";
import {
  parseThemePreference,
  type ResolvedTheme,
  type ThemePreference,
  type ThemeSaveState,
} from "../theme/themeModel";
import {
  APPEARANCE_STORAGE_KEY,
  persistenceState,
  persistAppearance,
  persistTheme,
  readStoredTheme,
  readSystemTheme,
  resetPersistence,
  scheduleAppearancePersist,
} from "./settingsPersistence";

export interface SettingsStoreState {
  preferences: AppearancePreferences;
  appearanceSaveState: AppearanceSaveState;
  appearanceLoaded: boolean;
  appearanceLoadError: string | null;

  themePreference: ThemePreference;
  systemTheme: ResolvedTheme;
  themeSaveState: ThemeSaveState;
  themeLoadError: string | null;

  updatePreferences: (partial: Partial<AppearancePreferences>) => void;
  setThemePreference: (preference: ThemePreference) => void;
  loadAppearance: () => Promise<void>;
  loadTheme: () => Promise<void>;
  retryAppearanceLoad: () => void;
  retryAppearanceSave: () => void;
  retryThemeLoad: () => void;
  retryThemeSave: () => void;
  syncSystemTheme: () => void;
  reset: () => void;
}

function createInitialState() {
  return {
    preferences: defaultAppearancePreferences,
    appearanceSaveState: "idle" as AppearanceSaveState,
    appearanceLoaded: false,
    appearanceLoadError: null as string | null,
    themePreference: "light" as ThemePreference,
    systemTheme: readSystemTheme(),
    themeSaveState: "idle" as ThemeSaveState,
    themeLoadError: null as string | null,
  };
}

export const useSettingsStore = create<SettingsStoreState>((set, get) => ({
  ...createInitialState(),

  updatePreferences: (partial) => {
    const next = normalizePreferences({ ...persistenceState.pendingPreferences, ...partial });
    persistenceState.pendingPreferences = next;
    set({ preferences: next });
    scheduleAppearancePersist(next, (state) => set({ appearanceSaveState: state }));
  },

  setThemePreference: (preference) => {
    persistenceState.themeUserChanged = true;
    persistenceState.pendingTheme = preference;
    set({ themePreference: preference });
    persistTheme(preference, (state) => set({ themeSaveState: state }));
  },

  loadAppearance: async () => {
    set({ appearanceLoadError: null });
    if (!isDesktopRuntime()) {
      try {
        const raw = window.localStorage.getItem(APPEARANCE_STORAGE_KEY);
        if (raw) {
          const next = normalizePreferences(JSON.parse(raw));
          persistenceState.pendingPreferences = next;
          set({ preferences: next });
        }
      } catch {
        set({ appearanceLoadError: "无法读取外观设置，请重试" });
      } finally {
        set({ appearanceLoaded: true });
      }
      return;
    }
    try {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          const raw = await desktop.getSetting("ui.preferences");
          if (!raw) return;
          try {
            const next = normalizePreferences(JSON.parse(raw));
            persistenceState.pendingPreferences = next;
            set({ preferences: next });
          } catch {
            set({ appearanceLoadError: "外观设置格式无效，已使用默认值" });
          }
          return;
        } catch {
          if (attempt < 2) await new Promise((resolve) => window.setTimeout(resolve, 150));
          else set({ appearanceLoadError: "无法读取外观设置，请重试" });
        }
      }
    } finally {
      set({ appearanceLoaded: true });
    }
  },

  loadTheme: async () => {
    set({ themeLoadError: null });
    const desktopRuntime = isDesktopRuntime();
    try {
      const value = await desktop.getSetting("ui.theme");
      if (persistenceState.themeUserChanged) return;
      // 桌面设置是权威值；仅在没有桥接值的浏览器预览里才用 localStorage，
      // 避免陈旧的预览值覆盖已被清空的桌面偏好。
      const next = value ? parseThemePreference(value) : desktopRuntime ? "light" : readStoredTheme();
      persistenceState.pendingTheme = next;
      set({ themePreference: next });
    } catch {
      if (persistenceState.themeUserChanged) return;
      const next = desktopRuntime ? "light" : readStoredTheme();
      persistenceState.pendingTheme = next;
      set({ themePreference: next });
      if (desktopRuntime) set({ themeLoadError: "无法读取主题设置，请重试" });
    }
  },

  retryAppearanceLoad: () => {
    void get().loadAppearance();
  },
  retryAppearanceSave: () => {
    persistAppearance(persistenceState.pendingPreferences, (state) => set({ appearanceSaveState: state }));
  },
  retryThemeLoad: () => {
    void get().loadTheme();
  },
  retryThemeSave: () => {
    persistTheme(persistenceState.pendingTheme, (state) => set({ themeSaveState: state }));
  },

  syncSystemTheme: () => {
    set({ systemTheme: readSystemTheme() });
  },

  reset: () => {
    resetPersistence();
    set({ ...createInitialState(), appearanceLoaded: true });
  },
}));

/** 从 `suna:setting-changed` 事件同步外观偏好。 */
export function applyExternalAppearance(raw: string): void {
  try {
    const next = normalizePreferences(JSON.parse(raw));
    persistenceState.pendingPreferences = next;
    useSettingsStore.setState({ preferences: next, appearanceLoadError: null, appearanceSaveState: "saved" });
  } catch {
    useSettingsStore.setState({ appearanceLoadError: "外观设置格式无效，已使用默认值" });
  }
}

/** 从 `suna:setting-changed` 事件同步主题。 */
export function applyExternalTheme(raw: string): void {
  persistenceState.themeUserChanged = true;
  const next = parseThemePreference(raw);
  persistenceState.pendingTheme = next;
  useSettingsStore.setState({ themePreference: next, themeLoadError: null, themeSaveState: "saved" });
}

/**
 * 懒初始化：Provider 与消费方 hook 都会调用，但真正的加载只发生一次。
 *
 * 迁移前这些状态由 React Context 提供，「没有 Provider」时消费方拿到的是已加载的
 * 默认值。迁移到全局 store 后没有这个隐式回退，因此改由 hook 在挂载时触发一次加载，
 * 保证隔离预览与单元测试的语义不变。
 */
export function ensureAppearanceLoaded(): void {
  if (persistenceState.appearanceLoadStarted) return;
  persistenceState.appearanceLoadStarted = true;
  void useSettingsStore.getState().loadAppearance();
}

/** 仅供测试：把 store 与模块级加载标记复位到初始状态。 */
export function resetSettingsStoreForTests(): void {
  useSettingsStore.getState().reset();
}
