import { getSettingValue, parseObject, setSettingValue } from "./settingsModel";

/** `model.preferences` 设置键：Temperature / Max Tokens / Context / 超时。 */
export const MODEL_PREFERENCES_KEY = "model.preferences";

export type ModelRuntimePreferences = {
  temperature: number;
  maxTokens: number;
  contextLength: number;
  timeoutSeconds: number;
};

export const modelRuntimeDefaults: ModelRuntimePreferences = {
  temperature: 0.2,
  maxTokens: 4096,
  contextLength: 32768,
  timeoutSeconds: 120,
};

export function normalizeModelRuntime(value: Partial<ModelRuntimePreferences>): ModelRuntimePreferences {
  const temperature = Number.isFinite(value.temperature) ? value.temperature ?? modelRuntimeDefaults.temperature : modelRuntimeDefaults.temperature;
  const maxTokens = Number.isFinite(value.maxTokens) ? value.maxTokens ?? modelRuntimeDefaults.maxTokens : modelRuntimeDefaults.maxTokens;
  const contextLength = Number.isFinite(value.contextLength) ? value.contextLength ?? modelRuntimeDefaults.contextLength : modelRuntimeDefaults.contextLength;
  const timeoutSeconds = Number.isFinite(value.timeoutSeconds) ? value.timeoutSeconds ?? modelRuntimeDefaults.timeoutSeconds : modelRuntimeDefaults.timeoutSeconds;
  return {
    temperature: Math.min(2, Math.max(0, temperature)),
    maxTokens: Math.min(262_144, Math.max(256, Math.round(maxTokens))),
    contextLength: Math.min(1_048_576, Math.max(1_024, Math.round(contextLength))),
    timeoutSeconds: Math.min(1_800, Math.max(5, Math.round(timeoutSeconds))),
  };
}

export function parseModelRuntime(raw: string | null | undefined): ModelRuntimePreferences {
  const parsed = parseObject(raw ?? null);
  return normalizeModelRuntime({
    temperature: typeof parsed.temperature === "number" ? parsed.temperature : modelRuntimeDefaults.temperature,
    maxTokens: typeof parsed.max_tokens === "number" ? parsed.max_tokens : modelRuntimeDefaults.maxTokens,
    contextLength: typeof parsed.context_length === "number" ? parsed.context_length : modelRuntimeDefaults.contextLength,
    timeoutSeconds: typeof parsed.timeout_seconds === "number" ? parsed.timeout_seconds : modelRuntimeDefaults.timeoutSeconds,
  });
}

export function serializeModelRuntime(value: ModelRuntimePreferences): string {
  return JSON.stringify({
    version: 1,
    temperature: value.temperature,
    max_tokens: value.maxTokens,
    context_length: value.contextLength,
    timeout_seconds: value.timeoutSeconds,
  });
}

export function loadModelRuntime(): Promise<ModelRuntimePreferences> {
  return getSettingValue(MODEL_PREFERENCES_KEY).then(parseModelRuntime);
}

export function saveModelRuntime(value: ModelRuntimePreferences): Promise<void> {
  return setSettingValue(MODEL_PREFERENCES_KEY, serializeModelRuntime(value));
}
