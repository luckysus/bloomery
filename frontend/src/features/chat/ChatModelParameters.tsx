import { useEffect, useRef, useState } from "react";
import { Check, LoaderCircle } from "lucide-react";
import { Input } from "../../components/ui/input";
import { settingsErrorMessage } from "../settings/settingsError";
import { loadModelRuntime, normalizeModelRuntime, saveModelRuntime, type ModelRuntimePreferences } from "../settings/modelPreferences";

/**
 * 第 19 章：聊天输入框附近的模型参数面板。
 * Provider / Model 来自当前对话 Profile；Temperature / Context / Max Tokens
 * 复用全局 `model.preferences`（应用于新的 Agent 运行）。
 */
export default function ChatModelParameters({ providerLabel, modelLabel }: { providerLabel: string; modelLabel: string }) {
  const [value, setValue] = useState<ModelRuntimePreferences | null>(null);
  const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    let mounted = true;
    void loadModelRuntime()
      .then((parsed) => { if (mounted) setValue(parsed); })
      .catch((cause) => { if (mounted) { setState("error"); setError(settingsErrorMessage(cause, "无法读取模型参数")); } });
    return () => { mounted = false; };
  }, []);

  const update = async (next: Partial<ModelRuntimePreferences>) => {
    if (!value) return;
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
      if (request === requestRef.current) { setState("error"); setError(settingsErrorMessage(cause, "模型参数保存失败")); }
    }
  };

  return (
    <div className="suna-chat-params" role="group" aria-label="模型参数">
      <div className="suna-chat-params-head">
        <strong>模型参数</strong>
        {state === "saving" && <span role="status"><LoaderCircle size={12} className="suna-spin" />保存中</span>}
        {state === "saved" && <span role="status"><Check size={12} />已保存</span>}
        {state === "error" && <span role="alert">{error}</span>}
      </div>
      <p className="suna-chat-params-meta"><span>Provider</span><em>{providerLabel}</em></p>
      <p className="suna-chat-params-meta"><span>Model</span><em>{modelLabel}</em></p>
      <label className="suna-chat-params-field"><span>Temperature</span>
        <Input aria-label="Temperature" type="number" min="0" max="2" step="0.1" disabled={!value} value={value?.temperature ?? ""} onChange={(event) => void update({ temperature: Number(event.target.value) })} />
      </label>
      <label className="suna-chat-params-field"><span>Context</span>
        <Input aria-label="Context" type="number" min="1024" max="1048576" step="1024" disabled={!value} value={value?.contextLength ?? ""} onChange={(event) => void update({ contextLength: Number(event.target.value) })} />
      </label>
      <label className="suna-chat-params-field"><span>Max Tokens</span>
        <Input aria-label="Max Tokens" type="number" min="256" max="262144" step="256" disabled={!value} value={value?.maxTokens ?? ""} onChange={(event) => void update({ maxTokens: Number(event.target.value) })} />
      </label>
    </div>
  );
}
