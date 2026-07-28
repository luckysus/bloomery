import { getSetting, setSetting } from "./settings";
import { isTauriRuntime } from "./tauri";

const LOCAL_LLM_CONFIG_KEY = "local_llm_config";

export type DesktopLlmConfigInput = {
  provider?: string;
  base_url?: string;
  model_name?: string;
  api_key?: string | null;
};

export type DesktopLlmConfig = Required<DesktopLlmConfigInput>;

function parseConfig(raw: string | null): DesktopLlmConfig {
  if (!raw) return { provider: "", base_url: "", model_name: "", api_key: "" };
  try {
    const value = JSON.parse(raw) as DesktopLlmConfigInput;
    return {
      provider: String(value.provider || ""),
      base_url: String(value.base_url || ""),
      model_name: String(value.model_name || ""),
      api_key: String(value.api_key || ""),
    };
  } catch {
    return { provider: "", base_url: "", model_name: "", api_key: "" };
  }
}

export async function saveDesktopLlmConfig(input: DesktopLlmConfigInput) {
  if (!isTauriRuntime()) return;
  const existing = parseConfig(await getSetting(LOCAL_LLM_CONFIG_KEY).catch(() => null));
  const next = {
    provider: input.provider ?? existing.provider,
    base_url: input.base_url ?? existing.base_url,
    model_name: input.model_name ?? existing.model_name,
    api_key: input.api_key && input.api_key.trim() ? input.api_key.trim() : existing.api_key,
  };
  await setSetting(LOCAL_LLM_CONFIG_KEY, JSON.stringify(next));
}

export async function loadDesktopLlmConfig() {
  if (!isTauriRuntime()) return null;
  return parseConfig(await getSetting(LOCAL_LLM_CONFIG_KEY).catch(() => null));
}
