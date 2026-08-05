import { describe, expect, it } from "vitest";
import { formatMessage, resolveLocale, type LanguagePreference } from "./locale";

describe("locale helpers", () => {
  it("follows English system locales and falls back to Chinese", () => {
    expect(resolveLocale("system", "en-US")).toBe("en-US");
    expect(resolveLocale("system", "zh-CN")).toBe("zh-CN");
    expect(resolveLocale("system", "fr-FR")).toBe("zh-CN");
  });

  it("honors an explicit language preference", () => {
    const preferences: LanguagePreference[] = ["zh-CN", "en-US"];
    expect(preferences.map((preference) => resolveLocale(preference, "en-US"))).toEqual(["zh-CN", "en-US"]);
  });

  it("replaces named message parameters", () => {
    expect(formatMessage("{count} documents", { count: 3 })).toBe("3 documents");
  });
});
