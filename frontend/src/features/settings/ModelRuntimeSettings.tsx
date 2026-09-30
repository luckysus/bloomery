import { useEffect, useState } from "react";
import { Check, SlidersHorizontal } from "lucide-react";
import { desktop } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import { parseObject } from "./settingsModel";

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

export default function ModelRuntimeSettings() {
  const { t } = useLocale();
  const [value, setValue] = useState(defaults);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    void desktop.getSetting("model.preferences").then((raw) => {
      const parsed = parseObject(raw);
      setValue({
        temperature: typeof parsed.temperature === "number" ? parsed.temperature : defaults.temperature,
        maxTokens: typeof parsed.max_tokens === "number" ? parsed.max_tokens : defaults.maxTokens,
        contextLength: typeof parsed.context_length === "number" ? parsed.context_length : defaults.contextLength,
        timeoutSeconds: typeof parsed.timeout_seconds === "number" ? parsed.timeout_seconds : defaults.timeoutSeconds,
      });
    });
  }, []);

  const update = async (next: Partial<ModelRuntimePreferences>) => {
    const merged = { ...value, ...next };
    setValue(merged);
    await desktop.setSetting("model.preferences", JSON.stringify({
      version: 1,
      temperature: merged.temperature,
      max_tokens: merged.maxTokens,
      context_length: merged.contextLength,
      timeout_seconds: merged.timeoutSeconds,
    }));
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1200);
  };

  return (
    <section className="suna-settings-runtime" aria-labelledby="model-runtime-heading">
      <div className="suna-settings-runtime-heading">
        <div><span className="suna-settings-card-icon"><SlidersHorizontal size={16} aria-hidden="true" /></span><div><h2 id="model-runtime-heading">模型运行参数</h2><p>应用于新的 Agent 运行。Provider 和 API Key 仍按能力单独配置。</p></div></div>
        {saved && <span className="suna-settings-inline-saved" role="status"><Check size={14} aria-hidden="true" />{t("settingsSaved")}</span>}
      </div>
      <div className="suna-settings-runtime-grid">
        <label className="suna-settings-field"><span>Temperature</span><input type="number" min="0" max="2" step="0.1" value={value.temperature} onChange={(event) => void update({ temperature: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Max Tokens</span><input type="number" min="256" max="262144" step="256" value={value.maxTokens} onChange={(event) => void update({ maxTokens: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Context Length</span><input type="number" min="1024" max="1048576" step="1024" value={value.contextLength} onChange={(event) => void update({ contextLength: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Timeout (seconds)</span><input type="number" min="5" max="1800" step="5" value={value.timeoutSeconds} onChange={(event) => void update({ timeoutSeconds: Number(event.target.value) })} /></label>
      </div>
    </section>
  );
}
