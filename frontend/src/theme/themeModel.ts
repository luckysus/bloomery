/**
 * 主题的纯模型层。
 *
 * 从 `theme/theme.tsx` 抽出，供 `stores/settingsStore` 与 Provider 共用，
 * 避免 store ↔ Provider 之间的循环依赖。这里只放类型与纯函数。
 */

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type ThemeSaveState = "idle" | "saving" | "saved" | "error";

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
