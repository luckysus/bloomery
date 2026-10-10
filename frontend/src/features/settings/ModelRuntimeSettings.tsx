import { useEffect, useRef, useState } from "react";
import { Check, LoaderCircle, RotateCcw, SlidersHorizontal } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { settingsErrorMessage } from "./settingsError";
import { Input } from "../../components/ui/input";
import { loadModelRuntime, modelRuntimeDefaults, normalizeModelRuntime, saveModelRuntime, type ModelRuntimePreferences } from "./modelPreferences";

const defaults = modelRuntimeDefaults;

export default function ModelRuntimeSettings() {
  const { t } = useLocale();
  const [value, setValue] = useState(defaults);
  const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    let mounted = true;
    void loadModelRuntime().then((parsed) => {
      if (mounted) setValue(parsed);
    }).catch((cause) => { if (mounted) { setState("error"); setError(settingsErrorMessage(cause, "无法读取模型运行参数")); } }).finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, []);

  const update = async (next: Partial<ModelRuntimePreferences>) => {
    const merged = normalizeModelRuntime({ ...value, ...next });
    const request = ++requestRef.current;
    setValue(merged);
    setError(null);
    setState("saving");
    try {
      await saveModelRuntime(merged);
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
        <label className="suna-settings-field"><span>Temperature</span><Input disabled={loading} type="number" min="0" max="2" step="0.1" value={value.temperature} onChange={(event) => void update({ temperature: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Max Tokens</span><Input disabled={loading} type="number" min="256" max="262144" step="256" value={value.maxTokens} onChange={(event) => void update({ maxTokens: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Context Length</span><Input disabled={loading} type="number" min="1024" max="1048576" step="1024" value={value.contextLength} onChange={(event) => void update({ contextLength: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>单次模型请求超时（秒）</span><Input disabled={loading} type="number" min="5" max="1800" step="5" value={value.timeoutSeconds} onChange={(event) => void update({ timeoutSeconds: Number(event.target.value) })} /></label>
      </div>
      <footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={reset} disabled={loading || state === "saving"}><RotateCcw size={15} />恢复运行参数默认值</button></footer>
    </section>
  );
}
