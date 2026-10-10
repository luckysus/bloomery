/**
 * 界面语言的纯模型层。
 *
 * 从 `i18n/locale.tsx` 抽出，供 `stores/localeStore` 与 Provider 共用，
 * 避免 store ↔ Provider 之间的循环依赖。这里只放类型与纯函数，
 * 不依赖翻译字典。
 */

export type Locale = "zh-CN" | "en-US";
export type LanguagePreference = "system" | Locale;
export type LocaleSaveState = "idle" | "saving" | "saved" | "error";

export const LOCALE_STORAGE_KEY = "suna.ui.locale";

export function resolveLocale(preference: LanguagePreference, systemLocale = "zh-CN"): Locale {
  if (preference === "zh-CN" || preference === "en-US") return preference;
  return systemLocale.toLowerCase().startsWith("en") ? "en-US" : "zh-CN";
}

export function detectSystemLocale(): string {
  return typeof navigator !== "undefined" ? navigator.language : "zh-CN";
}

export function parsePreference(value: string | null): LanguagePreference {
  if (!value) return "zh-CN";
  try {
    const parsed = JSON.parse(value) as { preference?: unknown };
    return parsed.preference === "zh-CN" || parsed.preference === "en-US" ? parsed.preference : "zh-CN";
  } catch {
    return "zh-CN";
  }
}
