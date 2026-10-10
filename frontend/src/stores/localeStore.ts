/**
 * localeStore —— 承载界面语言状态。
 *
 * 第 4 章要求状态管理使用 Zustand；`i18n/locale.tsx` 原先用 React Context 持有
 * 语言偏好，现在改为本 store 承载，Provider 只保留「触发加载 + 事件同步」。
 */
import { create } from "zustand";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import {
  LOCALE_STORAGE_KEY,
  detectSystemLocale,
  parsePreference,
  resolveLocale,
  type LanguagePreference,
  type Locale,
  type LocaleSaveState,
} from "../i18n/localeModel";

export interface LocaleStoreState {
  preference: LanguagePreference;
  systemLocale: string;
  locale: Locale;
  saveState: LocaleSaveState;
  loadError: string | null;

  setPreference: (preference: LanguagePreference) => void;
  load: () => Promise<void>;
  retryLoad: () => void;
  retrySave: () => void;
  applyExternal: (raw: string) => void;
  resetToDefault: () => void;
  reset: () => void;
}

let pendingPreference: LanguagePreference = "zh-CN";
let localeRequest = 0;
let localeLoadStarted = false;

function persist(next: LanguagePreference, set: (partial: Partial<LocaleStoreState>) => void) {
  const request = ++localeRequest;
  pendingPreference = next;
  set({ saveState: "saving" });
  const serialized = JSON.stringify({ version: 1, preference: next });
  try {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, serialized);
  } catch {
    // 本地存储不可用时，桌面桥的设置仍是权威值。
  }
  if (!isDesktopRuntime()) {
    if (request === localeRequest) set({ saveState: "saved" });
    return;
  }
  void desktop.setSetting("ui.locale", serialized).then(() => {
    if (request === localeRequest) set({ saveState: "saved" });
  }).catch(() => {
    if (request === localeRequest) set({ saveState: "error" });
  });
}

function createInitialState() {
  const systemLocale = detectSystemLocale();
  return {
    preference: "zh-CN" as LanguagePreference,
    systemLocale,
    locale: resolveLocale("zh-CN", systemLocale),
    saveState: "idle" as LocaleSaveState,
    loadError: null as string | null,
  };
}

export const useLocaleStore = create<LocaleStoreState>((set, get) => ({
  ...createInitialState(),

  setPreference: (next) => {
    set({ preference: next, locale: resolveLocale(next, get().systemLocale) });
    persist(next, set);
  },

  load: async () => {
    set({ loadError: null });
    if (!isDesktopRuntime()) {
      try {
        const next = parsePreference(window.localStorage.getItem(LOCALE_STORAGE_KEY));
        pendingPreference = next;
        set({ preference: next, locale: resolveLocale(next, get().systemLocale) });
      } catch {
        set({ loadError: "无法读取界面语言设置，请重试" });
      }
      return;
    }
    try {
      const value = await desktop.getSetting("ui.locale");
      const next = parsePreference(value);
      pendingPreference = next;
      set({ preference: next, locale: resolveLocale(next, get().systemLocale) });
    } catch {
      set({ loadError: "无法读取界面语言设置，请重试" });
    }
  },

  retryLoad: () => {
    void get().load();
  },

  retrySave: () => {
    persist(pendingPreference, set);
  },

  applyExternal: (raw) => {
    const next = parsePreference(raw);
    pendingPreference = next;
    set({
      preference: next,
      locale: resolveLocale(next, get().systemLocale),
      loadError: null,
      saveState: "saved",
    });
  },

  resetToDefault: () => {
    pendingPreference = "zh-CN";
    set({
      preference: "zh-CN",
      locale: resolveLocale("zh-CN", get().systemLocale),
      loadError: null,
      saveState: "saved",
    });
  },

  reset: () => {
    localeRequest += 1;
    localeLoadStarted = false;
    pendingPreference = "zh-CN";
    set(createInitialState());
  },
}));

/** 懒初始化：Provider 调用，真正的加载只发生一次。 */
export function ensureLocaleLoaded(): void {
  if (localeLoadStarted) return;
  localeLoadStarted = true;
  void useLocaleStore.getState().load();
}
