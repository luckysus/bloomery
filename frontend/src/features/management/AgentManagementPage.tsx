import { useEffect, useState } from "react";
import { Bot, Check, CircleAlert, Copy, LoaderCircle, RefreshCw, RotateCcw, Save, Shield, Trash2 } from "lucide-react";
import { desktop, type AgentPermissionRestrictions, type AgentProfileSummary, type ProviderProfileResponse, type ToolCapabilitySummary } from "../../bridge/desktop";
import { getSettingValue, parseObject } from "../settings/settingsModel";
import AgentSchedulesPanel from "../agents/AgentSchedulesPanel";
import "./management.css";
import "./agents.css";

const permissionLabels: Array<[keyof AgentPermissionRestrictions, string]> = [
  ["allowFileAccess", "文件读写"], ["allowShell", "Shell 执行"], ["allowNetwork", "网络访问"],
  ["allowDatabase", "数据库访问"], ["allowMcp", "MCP 工具"], ["confirmDangerous", "危险操作强制确认"],
];

const limitFields = [
  { key: "maxTurns", label: "最大循环次数", min: 1, max: 100 },
  { key: "maxToolCalls", label: "最大工具调用", min: 1, max: 1000 },
  { key: "contextBudget", label: "上下文预算 Token", min: 1024, max: 262144 },
  { key: "retries", label: "网络重试次数", min: 0, max: 10 },
  { key: "recoveryRetries", label: "检查点恢复次数", min: 0, max: 10 },
  { key: "runTimeoutSeconds", label: "整次运行期限（秒）", min: 30, max: 86400 },
] as const;

function reasonMessage(reason: unknown, fallback: string) {
  return reason instanceof Error ? reason.message : typeof reason === "string" ? reason : fallback;
}

function copyProfile(profile: AgentProfileSummary): AgentProfileSummary {
  return { ...profile, toolIds: [...profile.toolIds], permissionRestrictions: { ...profile.permissionRestrictions }, limits: { ...profile.limits } };
}

export default function AgentManagementPage() {
  const [agents, setAgents] = useState<AgentProfileSummary[]>([]);
  const [presets, setPresets] = useState<AgentProfileSummary[]>([]);
  const [providers, setProviders] = useState<ProviderProfileResponse[]>([]);
  const [tools, setTools] = useState<ToolCapabilitySummary[]>([]);
  const [mcpTools, setMcpTools] = useState<ToolCapabilitySummary[]>([]);
  const [globalPermissions, setGlobalPermissions] = useState<Record<string, unknown>>({});
  const [draft, setDraft] = useState<AgentProfileSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [loadNonce, setLoadNonce] = useState(0);
  const [templateId, setTemplateId] = useState("knowledge");

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    setMcpTools([]);
    setError("");
    void Promise.all([
      desktop.listAgentProfiles(), desktop.listAgentProfilePresets(), desktop.listProviderProfiles(),
      desktop.listToolCapabilities(), getSettingValue("agent.preferences"),
    ]).then(([profiles, templates, providerProfiles, capabilities, preferences]) => {
      if (!mounted) return;
      setAgents(profiles);
      setPresets(templates);
      setProviders(providerProfiles.filter((provider) => provider.enabled && provider.kind !== "mineru"));
      setTools(capabilities);
      setGlobalPermissions(parseObject(preferences));
      setDraft(profiles[0] ? copyProfile(profiles[0]) : null);
    }).catch((reason) => { if (mounted) setError(reasonMessage(reason, "无法加载 Agent 配置")); })
      .finally(() => { if (mounted) setLoading(false); });
    // MCP tools are discovered from configured servers; unavailable servers do not invent capabilities.
    void desktop.listMcpServers().then(async (servers) => {
      const results = await Promise.allSettled(servers.filter((server) => server.enabled).map(async (server) => {
        const entries = await desktop.listMcpTools(server.id);
        return entries.map((tool): ToolCapabilitySummary => ({ ...tool, enabled: true, source: server.display_name }));
      }));
      if (!mounted) return;
      const external = results.flatMap((result) => result.status === "fulfilled" ? result.value : []);
      setMcpTools(external);
    }).catch(() => undefined);
    return () => { mounted = false; };
  }, [loadNonce]);

  const update = (next: Partial<AgentProfileSummary>) => {
    setDraft((current) => current ? { ...current, ...next } : current);
    setSaved(false);
  };

  const applySaved = (profile: AgentProfileSummary) => {
    setAgents((current) => current.some((item) => item.id === profile.id)
      ? current.map((item) => item.id === profile.id ? profile : item) : [...current, profile]);
    setDraft(copyProfile(profile));
    setSaved(true);
  };

  const save = async () => {
    if (!draft) return;
    setBusy(true); setError("");
    try { applySaved(await desktop.saveAgentProfile(draft)); }
    catch (reason) { setError(reasonMessage(reason, "Agent 保存失败")); }
    finally { setBusy(false); }
  };

  const reset = async () => {
    if (!draft?.preset) return;
    setBusy(true); setError("");
    try { applySaved(await desktop.resetAgentProfile(draft.id)); }
    catch (reason) { setError(reasonMessage(reason, "恢复预设失败")); }
    finally { setBusy(false); }
  };

  const remove = async () => {
    if (!draft || draft.preset) return;
    if (!agents.some((agent) => agent.id === draft.id)) { setDraft(agents[0] ? copyProfile(agents[0]) : null); return; }
    setBusy(true); setError("");
    try {
      await desktop.deleteAgentProfile(draft.id);
      const remaining = agents.filter((agent) => agent.id !== draft.id);
      setAgents(remaining); setDraft(remaining[0] ? copyProfile(remaining[0]) : null); setSaved(false);
    } catch (reason) { setError(reasonMessage(reason, "删除 Agent 失败")); }
    finally { setBusy(false); }
  };

  const create = () => {
    const template = presets.find((profile) => profile.id === templateId);
    if (!template) return;
    setDraft({ ...copyProfile(template), id: `custom-${crypto.randomUUID().slice(0, 8)}`, name: `${template.name} 副本`, preset: false });
    setSaved(false); setError("");
  };

  const selectedSaved = agents.find((agent) => agent.id === draft?.id);
  const dirty = !!draft && JSON.stringify(draft) !== JSON.stringify(selectedSaved);
  const knownTools = [...tools, ...mcpTools.filter((entry, index) => !tools.some((tool) => tool.id === entry.id) && mcpTools.findIndex((tool) => tool.id === entry.id) === index)];
  const availableTools = [...knownTools, ...(draft?.toolIds ?? []).filter((id) => !knownTools.some((tool) => tool.id === id)).map((id) => ({ id, name: id, description: "当前未接入；运行时仅允许父任务已有工具", enabled: false, source: "saved" }))];

  return <section className="suna-management-page">
    <header className="suna-management-header"><div><span className="suna-module-kicker">SUNA AGENT CENTER</span><h1><Bot size={24} />Agent 管理</h1><p>为主 Agent 和专业专家配置独立职责、模型、工具与执行边界。</p></div><button type="button" className="suna-action-secondary" disabled={loading || busy} onClick={() => setLoadNonce((value) => value + 1)}><RefreshCw size={14} />刷新</button></header>
    {error && <div className="suna-management-alert" role="alert"><CircleAlert size={15} />{error}</div>}
    {loading ? <p className="suna-agents-loading" role="status"><LoaderCircle size={16} className="suna-spin" />加载 Agent 配置...</p> : <div className="suna-agents-layout">
      <aside className="suna-agents-sidebar" aria-label="专家 Agent 列表">
        <div className="suna-agents-create"><label><span>预设模板</span><select aria-label="预设模板" value={templateId} onChange={(event) => setTemplateId(event.target.value)} disabled={busy}>{presets.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}</select></label><button type="button" className="suna-action-secondary" disabled={busy || presets.length === 0} onClick={create}><Copy size={14} />创建自定义 Agent</button></div>
        {agents.map((agent) => <button type="button" className={`suna-agent-select ${draft?.id === agent.id ? "is-active" : ""}`} key={agent.id} disabled={busy} onClick={() => { setDraft(copyProfile(agent)); setSaved(false); setError(""); }} aria-pressed={draft?.id === agent.id}><span><Bot size={16} /><strong>{agent.name}</strong></span><small>{agent.description}</small><span className={`suna-management-badge ${agent.enabled ? "" : "is-muted"}`}>{agent.enabled ? <><Check size={12} />已启用</> : "已停用"} · {agent.preset ? "预设" : "自定义"}</span></button>)}
      </aside>
      {draft ? <div className="suna-agent-main"><form className="suna-agent-editor" onSubmit={(event) => { event.preventDefault(); void save(); }}>
        <header><div><h2>{draft.name}</h2><small>{draft.preset ? "预设专家，可恢复默认配置" : "自定义专家"} · {draft.id}</small></div><label className="suna-agent-enable"><input type="checkbox" checked={draft.enabled} disabled={busy} onChange={(event) => update({ enabled: event.target.checked, status: event.target.checked ? "available" : "disabled" })} />启用此 Agent</label></header>
        <fieldset disabled={busy}>
          <div className="suna-agent-fields"><label><span>Agent 名称</span><input required maxLength={80} value={draft.name} onChange={(event) => update({ name: event.target.value })} /></label><label><span>Provider / 模型</span><select value={draft.providerId ?? ""} onChange={(event) => update({ providerId: event.target.value || null })}><option value="">继承父任务 / 全局模型</option>{providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.display_name} · {provider.model_id || "默认模型"}</option>)}{draft.providerId && !providers.some((provider) => provider.id === draft.providerId) && <option value={draft.providerId}>当前 Provider 不可用</option>}</select></label></div>
          <label className="suna-agent-field"><span>职责说明</span><textarea rows={2} maxLength={2000} value={draft.description} onChange={(event) => update({ description: event.target.value })} /></label>
          <label className="suna-agent-field"><span>独立 System Prompt</span><textarea aria-label="独立 System Prompt" required rows={6} maxLength={16000} value={draft.systemPrompt} onChange={(event) => update({ systemPrompt: event.target.value })} /><small>与运行上下文一起应用于此专家；请明确证据标准、交付内容和职责边界。</small></label>
          <section className="suna-agent-section"><h3><Shield size={15} />工具白名单</h3><p>只允许调用选中的具体工具；实际工具还受父任务与全局配置限制。全部取消即禁用工具。</p><div className="suna-agent-tool-actions"><button type="button" className="suna-action-secondary" onClick={() => update({ toolIds: knownTools.filter((tool) => tool.enabled).map((tool) => tool.id) })}>选择已接入工具</button><button type="button" className="suna-action-secondary" onClick={() => update({ toolIds: [] })}>全部取消</button><small>{draft.toolIds.length} 项已选择</small></div><div className="suna-agent-tools">{availableTools.map((tool) => <label key={tool.id}><input type="checkbox" checked={draft.toolIds.includes(tool.id)} onChange={(event) => update({ toolIds: event.target.checked ? [...draft.toolIds, tool.id] : draft.toolIds.filter((id) => id !== tool.id) })} /><span><strong>{tool.name}</strong><code>{tool.id}</code><small>{tool.description}</small></span></label>)}</div></section>
          <section className="suna-agent-section"><h3><Shield size={15} />权限限制</h3><p>勾选表示允许使用父任务已授权的能力；专家不能扩大父任务或全局权限。关闭能力可进一步收窄范围。</p><div className="suna-agent-permissions">{permissionLabels.map(([key, label]) => <label key={key}><input type="checkbox" checked={draft.permissionRestrictions[key]} onChange={(event) => update({ permissionRestrictions: { ...draft.permissionRestrictions, [key]: event.target.checked } })} /><span>{label}{key !== "confirmDangerous" && globalPermissions[key] === false && <small>全局已禁用</small>}</span></label>)}</div></section>
          <section className="suna-agent-section"><h3>执行预算</h3><p>各项上限与父任务预算取更小值，子任务还受父任务剩余期限限制。</p><div className="suna-agent-fields">{limitFields.map(({ key, label, min, max }) => <label key={key}><span>{label}</span><input type="number" required min={min} max={max} step={1} value={draft.limits[key]} onChange={(event) => update({ limits: { ...draft.limits, [key]: Number(event.target.value) } })} /></label>)}</div></section>
        </fieldset>
        <footer><div><button type="submit" className="suna-primary-button" disabled={busy || !dirty}>{busy ? <LoaderCircle size={14} className="suna-spin" /> : <Save size={14} />}保存配置</button>{draft.preset ? <button type="button" className="suna-action-secondary" disabled={busy} onClick={() => void reset()}><RotateCcw size={14} />恢复此预设</button> : <button type="button" className="suna-action-secondary is-danger" disabled={busy} onClick={() => void remove()}><Trash2 size={14} />删除自定义 Agent</button>}</div><span role="status">{busy ? "保存中..." : dirty ? "有未保存修改" : saved ? "已保存" : "配置已加载"}</span></footer>
      </form><AgentSchedulesPanel agentId={selectedSaved?.id} /></div> : <div className="suna-management-empty">没有可编辑的 Agent，请刷新重试。</div>}
    </div>}
  </section>;
}
