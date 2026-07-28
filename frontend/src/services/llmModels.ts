import type { LLMModelInfo } from "../types/llm";

export const DEFAULT_LLM_MODELS: Record<string, LLMModelInfo[]> = {
  deepseek: [
    { id: "deepseek-v4-flash", name: "deepseek-v4-flash", provider: "DeepSeek" },
    { id: "deepseek-v4-pro", name: "deepseek-v4-pro", provider: "DeepSeek" },
  ],
  doubao: [
    { id: "doubao-seed-evolving", name: "Doubao-Seed-Evolving", provider: "字节跳动" },
    { id: "doubao-seed-2-1-turbo-260628", name: "Doubao-Seed-2.1-turbo", provider: "字节跳动" },
    { id: "doubao-seed-2-1-pro-260628", name: "Doubao-Seed-2.1-pro", provider: "字节跳动" },
    { id: "doubao-seed-2-0-mini-260428", name: "Doubao-Seed-2.0-mini", provider: "字节跳动" },
    { id: "doubao-seed-2-0-lite-260428", name: "Doubao-Seed-2.0-lite", provider: "字节跳动" },
    { id: "doubao-seed-2-0-pro-260215", name: "Doubao-Seed-2.0-pro", provider: "字节跳动" },
    { id: "doubao-seed-2-0-code-preview-260215", name: "Doubao-Seed-2.0-Code", provider: "字节跳动" },
    { id: "doubao-seed-1-8", name: "Doubao-Seed-1.8", provider: "字节跳动" },
    { id: "deepseek-v4-flash-260425", name: "DeepSeek-V4-flash", provider: "DeepSeek" },
    { id: "deepseek-v4-pro-260425", name: "DeepSeek-V4-pro", provider: "DeepSeek" },
  ],
};

const DOUBAO_CHAT_MODEL_ALLOWLIST = DEFAULT_LLM_MODELS.doubao.map((model) => ({
  ...model,
  key: normalizeVolcModelKey(model.id),
}));

export function normalizeVolcModelKey(modelId: string): string {
  return modelId
    .toLowerCase()
    .replace(/[-_](?:\d{6})(?=$|[-_])/g, "")
    .replace(/preview/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function filterDoubaoChatModels(models: LLMModelInfo[]): LLMModelInfo[] {
  const candidates = models.length ? models : DEFAULT_LLM_MODELS.doubao;
  return DOUBAO_CHAT_MODEL_ALLOWLIST.map((allowed) => {
    const matched = candidates
      .filter((model) => normalizeVolcModelKey(model.id || model.name || "") === allowed.key)
      .sort((a, b) => modelDateRank(b) - modelDateRank(a))[0];
    return matched ? { ...matched, name: allowed.name, provider: allowed.provider } : allowed;
  });
}

export function modelDateRank(model: LLMModelInfo): number {
  const text = `${model.id || ""} ${model.name || ""}`;
  const matches = Array.from(text.matchAll(/(?:^|[-_])(\d{6})(?=$|[-_])/g));
  if (!matches.length) return 0;
  return Math.max(...matches.map(match => {
    const value = match[1];
    const year = Number(value.slice(0, 2));
    const month = Number(value.slice(2, 4));
    const day = Number(value.slice(4, 6));
    if (!month || month > 12 || !day || day > 31) return 0;
    return 20000000 + year * 10000 + month * 100 + day;
  }));
}

export function sortDoubaoModelsByDate(models: LLMModelInfo[]): LLMModelInfo[] {
  return filterDoubaoChatModels(models)
    .map((model, index) => ({ model, index, rank: modelDateRank(model) }))
    .sort((a, b) => {
      if (a.rank !== b.rank) return b.rank - a.rank;
      if (a.rank > 0 && b.rank > 0) return String(a.model.id).localeCompare(String(b.model.id));
      return a.index - b.index;
    })
    .map(item => item.model);
}

export function formatChatModelButtonLabel(modelName: string, provider: string): string {
  const raw = modelName.trim();
  if (!raw) return "模型";
  const normalized = raw.toLowerCase();
  if (provider === "doubao" || normalized.includes("doubao-seed")) {
    if (normalized.includes("evolving")) return "Evolving";
    const version = normalized.match(/seed-(\d)-(\d)/);
    const tier = normalized.match(/-(turbo|mini|lite|pro|code)(?:-|$)/);
    const label = [
      version ? `${version[1]}.${version[2]}` : "",
      tier ? tier[1].replace(/^./, char => char.toUpperCase()) : "",
    ].filter(Boolean).join(" ");
    return label || "Doubao";
  }
  if (provider === "deepseek" || normalized.includes("deepseek")) {
    return raw
      .replace(/^deepseek-/i, "")
      .replace(/-/g, " ")
      .replace(/\b\w/g, char => char.toUpperCase())
      .replace(/\bV(\d)/g, "V$1")
      .slice(0, 18);
  }
  if (normalized.startsWith("gpt-")) {
    return raw.replace(/^gpt-/i, "");
  }
  return raw.length > 14 ? `${raw.slice(0, 12)}...` : raw;
}

export const LLM_MODEL_CACHE_TTL_MS = 5 * 60 * 1000;
