/**
 * 外观偏好的纯模型层。
 *
 * 从 `settings/appearance.tsx` 抽出，供 `stores/settingsStore` 与 Provider 共用，
 * 避免 store ↔ Provider 之间的循环依赖。这里只放类型、默认值与归一化逻辑，
 * 不含 React 或持久化副作用。
 */

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

export function normalizePreferences(raw: unknown): AppearancePreferences {
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
