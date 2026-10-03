import { useEffect, useRef, useState } from "react";
import { Check, LoaderCircle, RotateCcw, SlidersHorizontal } from "lucide-react";
import { desktop, isDesktopRuntime } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import { getSettingValue, parseObject, setSettingValue } from "./settingsModel";
import { settingsErrorMessage } from "./settingsError";

type ModelRuntimePreferences = {
  temperature: number;
  maxTokens: number;
  contextLength: number;
  timeoutSeconds: number;
};

const defaults: ModelRuntimePreferences = {
  temperature: 0.2,
  maxTokens: 4096,
  contextLength: 32768,
  timeoutSeconds: 120,
};

function normalize(value: Partial<ModelRuntimePreferences>): ModelRuntimePreferences {
  const temperature = Number.isFinite(value.temperature) ? value.temperature ?? defaults.temperature : defaults.temperature;
  const maxTokens = Number.isFinite(value.maxTokens) ? value.maxTokens ?? defaults.maxTokens : defaults.maxTokens;
  const contextLength = Number.isFinite(value.contextLength) ? value.contextLength ?? defaults.contextLength : defaults.contextLength;
  const timeoutSeconds = Number.isFinite(value.timeoutSeconds) ? value.timeoutSeconds ?? defaults.timeoutSeconds : defaults.timeoutSeconds;
  return {
    temperature: Math.min(2, Math.max(0, temperature)),
    maxTokens: Math.min(262_144, Math.max(256, Math.round(maxTokens))),
    contextLength: Math.min(1_048_576, Math.max(1_024, Math.round(contextLength))),
    timeoutSeconds: Math.min(1_800, Math.max(5, Math.round(timeoutSeconds))),
  };
}

export default function ModelRuntimeSettings() {
  const { t } = useLocale();
  const [value, setValue] = useState(defaults);
  const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    let mounted = true;
    void getSettingValue("model.preferences").then((raw) => {
      if (!mounted) return;
      const parsed = parseObject(raw);
      setValue(normalize({
        temperature: typeof parsed.temperature === "number" ? parsed.temperature : defaults.temperature,
        maxTokens: typeof parsed.max_tokens === "number" ? parsed.max_tokens : defaults.maxTokens,
        contextLength: typeof parsed.context_length === "number" ? parsed.context_length : defaults.contextLength,
        timeoutSeconds: typeof parsed.timeout_seconds === "number" ? parsed.timeout_seconds : defaults.timeoutSeconds,
      }));
    }).catch((cause) => { if (mounted) { setState("error"); setError(settingsErrorMessage(cause, "无法读取模型运行参数")); } }).finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, []);

  const update = async (next: Partial<ModelRuntimePreferences>) => {
    const merged = normalize({ ...value, ...next });
    const request = ++requestRef.current;
    setValue(merged);
    setError(null);
    setState("saving");
    try {
      await setSettingValue("model.preferences", JSON.stringify({
        version: 1,
        temperature: merged.temperature,
        max_tokens: merged.maxTokens,
        context_length: merged.contextLength,
        timeout_seconds: merged.timeoutSeconds,
      }));
      if (request === requestRef.current) {
        setState("saved");
        window.setTimeout(() => { if (request === requestRef.current) setState("idle"); }, 1200);
      }
    } catch (cause) {
      if (request === requestRef.current) {
        setState("error");
        setError(settingsErrorMessage(cause, "模型运行参数保存失败"));
      }
    }
  };

  const reset = () => { void update(defaults); };

  return (
    <section className="suna-settings-runtime" aria-labelledby="model-runtime-heading">
      <div className="suna-settings-runtime-heading">
        <div><span className="suna-settings-card-icon"><SlidersHorizontal size={16} aria-hidden="true" /></span><div><h2 id="model-runtime-heading">模型运行参数</h2><p>应用于新的 Agent 运行。Provider 和 API Key 仍按能力单独配置。</p></div></div>
        {loading && <span className="suna-settings-inline-saved" role="status"><LoaderCircle size={14} className="suna-spin" aria-hidden="true" />正在加载</span>}
        {!loading && state === "saving" && <span className="suna-settings-inline-saved" role="status"><LoaderCircle size={14} className="suna-spin" aria-hidden="true" />保存中</span>}
        {state === "saved" && <span className="suna-settings-inline-saved" role="status"><Check size={14} aria-hidden="true" />{t("settingsSaved")}</span>}
        {state === "error" && <span className="suna-settings-inline-error" role="alert">{error ?? "保存失败"}</span>}
      </div>
      <div className="suna-settings-runtime-grid">
        <label className="suna-settings-field"><span>Temperature</span><input disabled={loading} type="number" min="0" max="2" step="0.1" value={value.temperature} onChange={(event) => void update({ temperature: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Max Tokens</span><input disabled={loading} type="number" min="256" max="262144" step="256" value={value.maxTokens} onChange={(event) => void update({ maxTokens: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Context Length</span><input disabled={loading} type="number" min="1024" max="1048576" step="1024" value={value.contextLength} onChange={(event) => void update({ contextLength: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Timeout (seconds)</span><input disabled={loading} type="number" min="5" max="1800" step="5" value={value.timeoutSeconds} onChange={(event) => void update({ timeoutSeconds: Number(event.target.value) })} /></label>
      </div>
      <footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={reset} disabled={loading || state === "saving"}><RotateCcw size={15} />恢复运行参数默认值</button></footer>
    </section>
  );
}
