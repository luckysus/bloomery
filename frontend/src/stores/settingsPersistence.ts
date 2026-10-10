/**
 * 设置持久化的底层实现（供 `stores/settingsStore` 使用）。
 *
 * 抽出来的原因有二：一是 store 文件受 250 行预算约束；二是这些是纯副作用逻辑
 * （localStorage / 桌面桥 / 防抖 / 请求序号），与 store 的状态定义关注点不同。
 *
 * 持久化规则与迁移前完全一致：
 * - 桌面运行时以 `desktop.getSetting/setSetting` 为准；
 * - 浏览器预览降级到 localStorage；
 * - 外观偏好写入有 650ms 防抖，主题写入立即生效。
 */
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import {
  defaultAppearancePreferences,
  type AppearancePreferences,
} from "../settings/appearanceModel";
import { parseThemePreference, type ResolvedTheme, type ThemePreference } from "../theme/themeModel";

export const THEME_STORAGE_KEY = "suna.ui.theme";
export const APPEARANCE_STORAGE_KEY = "suna.ui.preferences";
const APPEARANCE_DEBOUNCE_MS = 650;

/** 模块级可变状态：迁移前由 Provider 的 ref 持有，现在集中在这里。 */
export const persistenceState = {
  pendingPreferences: defaultAppearancePreferences as AppearancePreferences,
  pendingTheme: "light" as ThemePreference,
  appearanceTimer: null as number | null,
  appearanceRequest: 0,
  themeRequest: 0,
  themeUserChanged: false,
  appearanceLoadStarted: false,
  themeLoadStarted: false,
};

export function resetPersistence(): void {
  persistenceState.pendingPreferences = defaultAppearancePreferences;
  persistenceState.pendingTheme = "light";
  persistenceState.appearanceRequest += 1;
  persistenceState.themeRequest += 1;
  persistenceState.themeUserChanged = false;
  persistenceState.appearanceLoadStarted = false;
  persistenceState.themeLoadStarted = false;
  if (persistenceState.appearanceTimer !== null) {
    window.clearTimeout(persistenceState.appearanceTimer);
    persistenceState.appearanceTimer = null;
  }
}

export function readSystemTheme(): ResolvedTheme {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export function readStoredTheme(): ThemePreference {
  if (typeof window === "undefined") return "light";
  try {
    return parseThemePreference(window.localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "light";
  }
}

export type SaveStateSetter = (state: "saving" | "saved" | "error") => void;

export function persistAppearance(next: AppearancePreferences, setSaveState: SaveStateSetter): void {
  const request = ++persistenceState.appearanceRequest;
  setSaveState("saving");
  if (!isDesktopRuntime()) {
    try {
      window.localStorage.setItem(APPEARANCE_STORAGE_KEY, JSON.stringify(next));
      if (request === persistenceState.appearanceRequest) setSaveState("saved");
    } catch {
      if (request === persistenceState.appearanceRequest) setSaveState("error");
    }
    return;
  }
  void desktop.setSetting("ui.preferences", JSON.stringify(next)).then(() => {
    if (request === persistenceState.appearanceRequest) setSaveState("saved");
  }).catch(() => {
    if (request === persistenceState.appearanceRequest) setSaveState("error");
  });
}

export function scheduleAppearancePersist(
  next: AppearancePreferences,
  setSaveState: SaveStateSetter,
): void {
  if (persistenceState.appearanceTimer !== null) {
    window.clearTimeout(persistenceState.appearanceTimer);
  }
  persistenceState.appearanceTimer = window.setTimeout(() => {
    persistenceState.appearanceTimer = null;
    persistAppearance(next, setSaveState);
  }, APPEARANCE_DEBOUNCE_MS);
}

export function persistTheme(next: ThemePreference, setSaveState: SaveStateSetter): void {
  const request = ++persistenceState.themeRequest;
  persistenceState.pendingTheme = next;
  setSaveState("saving");
  let localSaved = true;
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, JSON.stringify({ version: 1, preference: next }));
  } catch {
    localSaved = false;
    if (request === persistenceState.themeRequest) setSaveState("error");
  }
  // 桌面桥是权威存储；浏览器预览下它会无害地拒绝，此时以 localStorage 为准。
  if (!isDesktopRuntime()) {
    if (localSaved && request === persistenceState.themeRequest) setSaveState("saved");
    return;
  }
  void desktop.setSetting("ui.theme", JSON.stringify({ version: 1, preference: next }))
    .then(() => {
      if (request === persistenceState.themeRequest) setSaveState("saved");
    })
    .catch(() => {
      if (request === persistenceState.themeRequest) setSaveState("error");
    });
}
