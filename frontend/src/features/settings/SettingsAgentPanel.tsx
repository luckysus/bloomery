import { useEffect, useRef, useState } from "react";
import { Bot, Check, LoaderCircle, RefreshCw, RotateCcw, ShieldCheck } from "lucide-react";
import { desktop, isDesktopRuntime } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import { getSettingValue, parseObject, setSettingValue } from "./settingsModel";
import { settingsErrorMessage } from "./settingsError";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import { Textarea } from "../../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

type AgentSettings = {
  defaultAgent: string;
  autoSelectAgent: boolean;
  systemPrompt: string;
  maxTurns: number;
  maxToolCalls: number;
  contextBudget: number;
  retries: number;
  recoveryRetries: number;
  runTimeoutSeconds: number;
  workingDirectory: string;
  streamOutput: boolean;
  autoPlan: boolean;
  autoKnowledge: boolean;
  autoTools: boolean;
  saveCheckpoints: boolean;
  allowRecovery: boolean;
  confirmDangerous: boolean;
  allowFileAccess: boolean;
  allowShell: boolean;
  allowNetwork: boolean;
  allowDatabase: boolean;
  allowMcp: boolean;
};

const defaults: AgentSettings = {
  defaultAgent: "master",
  autoSelectAgent: true,
  systemPrompt: "你是 Suna 的钢铁材料研发智能体。回答必须区分事实、推断和待验证内容。回答按「结论 → 分析依据 → 知识来源 → 数据分析 → 模型结果 → 建议 → 引用」的顺序组织；只向用户呈现任务进度、Agent 状态、工具调用摘要与最终结果，不要暴露内部推理过程。",
  maxTurns: 20,
  maxToolCalls: 64,
  contextBudget: 32768,
  retries: 2,
  recoveryRetries: 2,
  runTimeoutSeconds: 1800,
  workingDirectory: "",
  streamOutput: true,
  autoPlan: true,
  autoKnowledge: true,
  autoTools: true,
  saveCheckpoints: true,
  allowRecovery: true,
  confirmDangerous: true,
  allowFileAccess: true,
  allowShell: false,
  allowNetwork: true,
  allowDatabase: true,
  allowMcp: true,
};

function booleanValue(value: unknown, fallback: boolean) {
  return typeof value === "boolean" ? value : fallback;
}

function numberValue(value: unknown, fallback: number, min: number, max: number) {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, Math.round(value))) : fallback;
}

export function normalizeAgentPreferences(raw: string | null): AgentSettings {
  const value = parseObject(raw);
  return {
    defaultAgent: typeof value.defaultAgent === "string" && value.defaultAgent.trim() ? value.defaultAgent.trim() : defaults.defaultAgent,
    autoSelectAgent: booleanValue(value.autoSelectAgent, defaults.autoSelectAgent),
    systemPrompt: typeof value.systemPrompt === "string" && value.systemPrompt.trim() ? value.systemPrompt.slice(0, 16000) : defaults.systemPrompt,
    maxTurns: numberValue(value.maxTurns, defaults.maxTurns, 1, 100),
    maxToolCalls: numberValue(value.maxToolCalls, defaults.maxToolCalls, 1, 1000),
    contextBudget: numberValue(value.contextBudget, defaults.contextBudget, 1024, 262144),
    retries: numberValue(value.retries, defaults.retries, 0, 10),
    recoveryRetries: numberValue(value.recoveryRetries, defaults.recoveryRetries, 0, 10),
    runTimeoutSeconds: numberValue(value.runTimeoutSeconds, defaults.runTimeoutSeconds, 30, 86400),
    workingDirectory: typeof value.workingDirectory === "string" ? value.workingDirectory.trim() : "",
    streamOutput: booleanValue(value.streamOutput, defaults.streamOutput),
    autoPlan: booleanValue(value.autoPlan, defaults.autoPlan),
    autoKnowledge: booleanValue(value.autoKnowledge, defaults.autoKnowledge),
    autoTools: booleanValue(value.autoTools, defaults.autoTools),
    saveCheckpoints: booleanValue(value.saveCheckpoints, defaults.saveCheckpoints),
    allowRecovery: booleanValue(value.allowRecovery, defaults.allowRecovery),
    confirmDangerous: booleanValue(value.confirmDangerous, defaults.confirmDangerous),
    allowFileAccess: booleanValue(value.allowFileAccess, defaults.allowFileAccess),
    allowShell: booleanValue(value.allowShell, defaults.allowShell),
    allowNetwork: booleanValue(value.allowNetwork, defaults.allowNetwork),
    allowDatabase: booleanValue(value.allowDatabase, defaults.allowDatabase),
    allowMcp: booleanValue(value.allowMcp, defaults.allowMcp),
  };
}

function Toggle({ checked, label, copy, onChange }: { checked: boolean; label: string; copy: string; onChange: (value: boolean) => void }) {
  return <label className="suna-settings-toggle-row"><span><strong>{label}</strong><small>{copy}</small></span><Checkbox aria-label={label} checked={checked} onCheckedChange={(value) => onChange(value === true)} /></label>;
}

export default function SettingsAgentPanel() {
  const { t } = useLocale();
  const [value, setValue] = useState(defaults);
  const [profiles, setProfiles] = useState<Array<{ id: string; name: string; description: string }>>([]);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [loadNonce, setLoadNonce] = useState(0);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(defaults);
  const timer = useRef<number | null>(null);

  const syncDangerousPreference = async (next: boolean) => {
    const current = parseObject(await getSettingValue("ui.preferences"));
    await setSettingValue("ui.preferences", JSON.stringify({ ...current, confirmDangerous: next }));
  };

  const persist = async (next: AgentSettings) => {
    setSaveState("saving");
    setLoadError(false);
    setError(null);
    try {
      await setSettingValue("agent.preferences", JSON.stringify({ version: 3, ...next }));
      await syncDangerousPreference(next.confirmDangerous);
      setSaveState("saved");
    } catch (cause) {
      setSaveState("error");
      setError(settingsErrorMessage(cause, "Agent 设置保存失败，请重试"));
    }
  };

  useEffect(() => {
    let mounted = true;
    setLoaded(false);
    setLoadError(false);
    setError(null);
    void getSettingValue("agent.preferences")
      .then((raw) => { if (mounted) { const next = normalizeAgentPreferences(raw); pending.current = next; setValue(next); setLoadError(false); } })
      .catch((cause) => { if (mounted) { setLoadError(true); setError(settingsErrorMessage(cause, "无法读取 Agent 设置，请重试")); } })
      .finally(() => { if (mounted) setLoaded(true); });
    if (isDesktopRuntime() && typeof desktop.listAgentProfiles === "function") {
      void desktop.listAgentProfiles().then((items) => { if (mounted) setProfiles(items.filter((item) => item.enabled && item.status === "available")); }).catch(() => undefined);
    }
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "agent.preferences" || !detail.value) return;
      const next = normalizeAgentPreferences(detail.value);
      pending.current = next;
      setValue(next);
      setSaveState("saved");
    };
    window.addEventListener("suna:setting-changed", changed);
    return () => {
      mounted = false;
      window.removeEventListener("suna:setting-changed", changed);
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [loadNonce]);

  const update = <K extends keyof AgentSettings>(key: K, nextValue: AgentSettings[K]) => {
    const next = normalizeAgentPreferences(JSON.stringify({ ...pending.current, [key]: nextValue }));
    pending.current = next;
    setValue(next);
    setSaveState("idle");
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { timer.current = null; void persist(next); }, 650);
  };

  const reset = () => {
    pending.current = defaults;
    setValue(defaults);
    setSaveState("idle");
    setError(null);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { timer.current = null; void persist(defaults); }, 0);
  };

  const profileOptions = profiles.length ? profiles : [{ id: value.defaultAgent, name: `当前配置（${value.defaultAgent}）`, description: "" }];
  return <section className="suna-settings-form-panel" aria-labelledby="settings-agent-heading">
    <header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">AGENT RUNTIME</span><h2 id="settings-agent-heading">{t("settingsCategoryAgent")}</h2><p>{t("settingsAgentCopy")}</p></div><Bot size={22} aria-hidden="true" /></header>
    {error && !loadError && <p className="suna-settings-inline-error" role="alert">{error}</p>}
    {!loaded || loadError ? <div className="suna-settings-state">{!loaded && <p className="suna-settings-loading" role="status"><LoaderCircle size={16} className="suna-spin" />加载 Agent 配置...</p>}{loadError && <p className="suna-settings-inline-error" role="alert">{error ?? "无法读取 Agent 设置，请重试"}<button type="button" className="suna-settings-inline-retry" onClick={() => setLoadNonce((nonce) => nonce + 1)}><RefreshCw size={13} />重试</button></p>}</div> : <>
      <div className="suna-settings-form-grid">
        <label className="suna-settings-field"><span>默认 Agent</span><Select value={value.defaultAgent} onValueChange={(value) => update("defaultAgent", value)}>
  <SelectTrigger aria-label="默认 Agent"><SelectValue /></SelectTrigger>
  <SelectContent>{profileOptions.map((profile) => <SelectItem key={profile.id} value={profile.id}>{profile.name}</SelectItem>)}
  </SelectContent>
</Select></label>
        <label className="suna-settings-field"><span>上下文预算 Token</span><Input type="number" min="1024" max="262144" step="1024" value={value.contextBudget} onChange={(event) => update("contextBudget", Number(event.target.value))} /></label>
        <label className="suna-settings-field"><span>最大循环次数</span><Input type="number" min="1" max="100" value={value.maxTurns} onChange={(event) => update("maxTurns", Number(event.target.value))} /></label>
        <label className="suna-settings-field"><span>最大工具调用次数</span><Input type="number" min="1" max="1000" value={value.maxToolCalls} onChange={(event) => update("maxToolCalls", Number(event.target.value))} /></label>
        <label className="suna-settings-field"><span>网络重试次数</span><Input type="number" min="0" max="10" value={value.retries} onChange={(event) => update("retries", Number(event.target.value))} /></label>
        <label className="suna-settings-field"><span>检查点恢复次数</span><Input type="number" min="0" max="10" value={value.recoveryRetries} onChange={(event) => update("recoveryRetries", Number(event.target.value))} /></label>
        <label className="suna-settings-field"><span>整次运行期限（秒）</span><Input type="number" min="30" max="86400" value={value.runTimeoutSeconds} onChange={(event) => update("runTimeoutSeconds", Number(event.target.value))} /></label>
      </div>
      <label className="suna-settings-field suna-settings-textarea-field"><span>Agent 工作目录</span><Input aria-label="Agent 工作目录" value={value.workingDirectory} placeholder="留空使用应用的 Agent 工作目录" onChange={(event) => update("workingDirectory", event.target.value)} /><small>文件读写和 Shell 仅允许在此目录内执行。留空使用应用专用目录。</small><button type="button" className="suna-secondary-button" onClick={() => void desktop.openFileDialog({ directory: true, multiple: false }).then((path) => { if (typeof path === "string") update("workingDirectory", path); }).catch((cause) => setError(settingsErrorMessage(cause, "无法选择工作目录")))}>选择文件夹</button></label>
      <label className="suna-settings-field suna-settings-textarea-field"><span>System Prompt</span><Textarea rows={4} value={value.systemPrompt} onChange={(event) => update("systemPrompt", event.target.value)} /></label>
      <div className="suna-settings-subsection"><div className="suna-settings-subsection-heading"><ShieldCheck size={17} /><div><strong>运行策略</strong><small>这些开关会直接影响 Agent Loop 的规划、知识检索、工具调用和输出行为。</small></div></div><div className="suna-settings-toggle-list">
        <Toggle checked={value.autoSelectAgent} label="自动选择 Agent" copy="根据任务类型选择最合适的 Agent 配置。" onChange={(next) => update("autoSelectAgent", next)} />
        <Toggle checked={value.autoPlan} label="自动规划" copy="允许 Agent 在执行前拆解任务并跟踪步骤。" onChange={(next) => update("autoPlan", next)} />
        <Toggle checked={value.autoKnowledge} label="自动调用知识库" copy="需要研究证据时自动加载知识检索工具。" onChange={(next) => update("autoKnowledge", next)} />
        <Toggle checked={value.autoTools} label="自动调用工具" copy="允许 Agent Loop 自动执行已授权的工具。" onChange={(next) => update("autoTools", next)} />
        <Toggle checked={value.streamOutput} label="流式输出" copy="边生成边显示模型回答和运行事件。" onChange={(next) => update("streamOutput", next)} />
      </div></div>
      <div className="suna-settings-subsection"><div className="suna-settings-subsection-heading"><ShieldCheck size={17} /><div><strong>运行与权限</strong><small>控制 Agent Loop 的恢复能力和工具边界。</small></div></div><div className="suna-settings-toggle-list">
        <Toggle checked={value.saveCheckpoints} label={t("settingsSaveCheckpoints")} copy="保存每次运行的检查点，便于查看和恢复。" onChange={(next) => update("saveCheckpoints", next)} />
        <Toggle checked={value.allowRecovery} label={t("settingsAllowRecovery")} copy="允许应用启动后恢复上次中断的任务。" onChange={(next) => update("allowRecovery", next)} />
        <Toggle checked={value.confirmDangerous} label="危险操作需要确认" copy="文件写入、Shell 和外部服务操作需要人工确认。" onChange={(next) => update("confirmDangerous", next)} />
        <Toggle checked={value.allowFileAccess} label="文件读写" copy="允许 Agent 读取和写入已授权的工作区文件。" onChange={(next) => update("allowFileAccess", next)} />
        <Toggle checked={value.allowShell} label="Shell 执行" copy="允许 Agent 调用本机命令行，默认关闭。" onChange={(next) => update("allowShell", next)} />
        <Toggle checked={value.allowNetwork} label="网络访问" copy="允许已配置的 Provider 和 MCP 服务发起请求。" onChange={(next) => update("allowNetwork", next)} />
        <Toggle checked={value.allowDatabase} label="数据库访问" copy="允许 Agent 查询已配置的生产数据和数据库连接。" onChange={(next) => update("allowDatabase", next)} />
        <Toggle checked={value.allowMcp} label="MCP 工具" copy="允许 Agent 使用已启用的 MCP Server 工具。" onChange={(next) => update("allowMcp", next)} />
      </div></div>
      <footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={reset}><RotateCcw size={15} />恢复默认 Agent 设置</button><span className={`suna-settings-inline-saved is-${saveState}`} role="status">{saveState === "saving" && <LoaderCircle size={14} className="suna-spin" />}{saveState === "saved" && <Check size={14} />}{saveState === "error" ? "保存失败，修改后重试" : saveState === "saving" ? "保存中" : saveState === "saved" ? "已保存" : "自动保存"}</span></footer>
    </>}
  </section>;
}
