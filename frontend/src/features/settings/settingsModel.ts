import type {
  ProviderKind,
  ProviderProfileResponse,
} from "../../bridge/desktop";

export type ProviderSlot = "chat" | "embedding" | "reranker" | "mineru";
export type RetrievalPlan = "free" | "pro";

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
}

export const defaultRetrievalIds: RetrievalIds = {
  embedding: null,
  reranker: null,
  mineru: null,
};

export const defaults: Record<ProviderSlot, Omit<SettingsEditor, "slot" | "id" | "apiKey" | "secretConfigured">> = {
  chat: {
    kind: "open_ai_compatible",
    displayName: "OpenAI Compatible",
    baseUrl: "https://api.openai.com/v1",
    modelId: "gpt-4o-mini",
    enabled: true,
  },
  embedding: {
    kind: "siliconflow",
    displayName: "SiliconFlow Embedding",
    baseUrl: "https://api.siliconflow.cn/v1",
    modelId: "BAAI/bge-m3",
    enabled: true,
  },
  reranker: {
    kind: "siliconflow",
    displayName: "SiliconFlow Reranker",
    baseUrl: "https://api.siliconflow.cn/v1",
    modelId: "BAAI/bge-reranker-v2-m3",
    enabled: true,
  },
  mineru: {
    kind: "mineru",
    displayName: "MinerU",
    baseUrl: "https://mineru.net/api/v4",
    modelId: "",
    enabled: true,
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
    if (slot === "chat") return profile.kind === "open_ai_compatible" || profile.kind === "ollama";
    if (slot === "mineru") return profile.kind === "mineru";
    if (profile.kind !== "siliconflow") return false;
    return slot === "embedding"
      ? profile.model_id?.toLowerCase().includes("bge-m3") && !profile.model_id.toLowerCase().includes("reranker")
      : profile.model_id?.toLowerCase().includes("rerank");
  });
}

export function editorFor(slot: ProviderSlot, profile: ProviderProfileResponse | undefined): SettingsEditor {
  const fallback = defaults[slot];
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
