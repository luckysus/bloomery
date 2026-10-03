import { desktop, isDesktopRuntime, type ProviderKind, type ProviderProfileResponse } from "../../bridge/desktop";

export type ProviderSlot = "chat" | "embedding" | "reranker" | "mineru";
export type RetrievalPlan = "free" | "pro";
const localSettingKey = (key: string) => `suna.setting.${key}`;
const localAliases: Record<string, string> = {
  "ui.theme": "suna.ui.theme",
  "ui.locale": "suna.ui.locale",
  "ui.preferences": "suna.ui.preferences",
};
export const settingChangedEvent = "suna:setting-changed";

export function readLocalSettingValue(key: string): string | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage.getItem(localSettingKey(key))
      ?? (localAliases[key] ? window.localStorage.getItem(localAliases[key]) : null);
  } catch {
    return null;
  }
}

function writeLocalSettingValue(key: string, value: string): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(localSettingKey(key), value);
  const alias = localAliases[key];
  if (alias) window.localStorage.setItem(alias, value);
}

export function getSettingValue(key: string): Promise<string | null> {
  return isDesktopRuntime() ? desktop.getSetting(key) : Promise.resolve(readLocalSettingValue(key));
}

export async function setSettingValue(key: string, value: string): Promise<void> {
  if (isDesktopRuntime()) {
    await desktop.setSetting(key, value);
  } else {
    try {
      writeLocalSettingValue(key, value);
    } catch (error) {
      throw error instanceof Error ? error : new Error("本地设置不可用");
    }
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(settingChangedEvent, { detail: { key, value } }));
  }
}

export interface RetrievalIds {
  embedding: string | null;
  reranker: string | null;
  mineru: string | null;
}

export interface SettingsEditor {
  slot: ProviderSlot;
  id: string | null;
  kind: ProviderKind;
  displayName: string;
  baseUrl: string;
  modelId: string;
  apiKey: string;
  enabled: boolean;
  secretConfigured: boolean;
  temperature: number;
  maxTokens: number;
}

export type ProviderRuntimeMeta = { temperature?: number; max_tokens?: number };

export const defaultRetrievalIds: RetrievalIds = {
  embedding: null,
  reranker: null,
  mineru: null,
};

export const defaults: Record<ProviderSlot, Omit<SettingsEditor, "slot" | "id" | "apiKey" | "secretConfigured">> = {
  chat: {
    // An absent profile is an unconfigured capability. Provider presets are
    // applied only after the user selects a provider in the editor; keeping
    // these fields empty prevents the settings page from presenting demo
    // credentials/configuration as if a model were already connected.
    kind: "open_ai_compatible",
    displayName: "",
    baseUrl: "",
    modelId: "",
    enabled: false,
    temperature: 0.2,
    maxTokens: 4096,
  },
  embedding: {
    kind: "siliconflow",
    displayName: "",
    baseUrl: "",
    modelId: "",
    enabled: false,
    temperature: 0.2,
    maxTokens: 4096,
  },
  reranker: {
    kind: "siliconflow",
    displayName: "",
    baseUrl: "",
    modelId: "",
    enabled: false,
    temperature: 0.2,
    maxTokens: 4096,
  },
  mineru: {
    kind: "mineru",
    displayName: "",
    baseUrl: "",
    modelId: "",
    enabled: false,
    temperature: 0.2,
    maxTokens: 4096,
  },
};

export const slotTitles: Record<ProviderSlot, "settingsChatProvider" | "settingsEmbeddingProvider" | "settingsRerankerProvider" | "settingsMineruProvider"> = {
  chat: "settingsChatProvider",
  embedding: "settingsEmbeddingProvider",
  reranker: "settingsRerankerProvider",
  mineru: "settingsMineruProvider",
};

export function parseObject(value: string | null) {
  if (!value) return {} as Record<string, unknown>;
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" ? parsed as Record<string, unknown> : {};
  } catch {
    return {} as Record<string, unknown>;
  }
}

export function parseId(value: unknown) {
  return typeof value === "string" && value.trim() ? value : null;
}

export function profileForSlot(
  slot: ProviderSlot,
  profiles: ProviderProfileResponse[],
  completed: Record<string, unknown>,
  retrieval: Record<string, unknown>,
) {
  const configuredId = slot === "chat"
    ? parseId(completed.llm_profile_id)
    : parseId(retrieval[`${slot}_profile_id`]);
  const byId = configuredId ? profiles.find((profile) => profile.id === configuredId) : undefined;
  if (byId) return byId;

  return profiles.find((profile) => {
    if (slot === "chat") {
      return profile.kind === "deepseek"
        || profile.kind === "anthropic"
        || profile.kind === "qwen"
        || profile.kind === "open_ai_compatible"
        || profile.kind === "ollama";
    }
    if (slot === "mineru") return profile.kind === "mineru";
    if (profile.kind !== "siliconflow") return false;
    return slot === "embedding"
      ? profile.model_id?.toLowerCase().includes("bge-m3") && !profile.model_id.toLowerCase().includes("reranker")
      : profile.model_id?.toLowerCase().includes("rerank");
  });
}

export function editorFor(slot: ProviderSlot, profile: ProviderProfileResponse | undefined, meta: ProviderRuntimeMeta = {}): SettingsEditor {
  const fallback = defaults[slot];
  const temperature = typeof meta.temperature === "number" && Number.isFinite(meta.temperature)
    ? Math.min(2, Math.max(0, meta.temperature))
    : fallback.temperature;
  const maxTokens = typeof meta.max_tokens === "number" && Number.isFinite(meta.max_tokens)
    ? Math.min(262_144, Math.max(256, Math.round(meta.max_tokens)))
    : fallback.maxTokens;
  return {
    slot,
    id: profile?.id ?? null,
    kind: profile?.kind ?? fallback.kind,
    displayName: profile?.display_name ?? fallback.displayName,
    baseUrl: profile?.base_url ?? fallback.baseUrl,
    modelId: profile?.model_id ?? fallback.modelId,
    apiKey: "",
    enabled: profile?.enabled ?? fallback.enabled,
    secretConfigured: profile?.secret_configured ?? false,
    temperature,
    maxTokens,
  };
}

export function providerErrorMessage(
  code: string | null | undefined,
  translate: (key: "credentialAuthentication" | "providerQuota" | "providerTimeout" | "providerNetwork" | "providerInvalidResponse") => string,
) {
  switch (code) {
    case "authentication":
      return translate("credentialAuthentication");
    case "quota":
      return translate("providerQuota");
    case "timeout":
      return translate("providerTimeout");
    case "network":
      return translate("providerNetwork");
    default:
      return translate("providerInvalidResponse");
  }
}

export function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}
