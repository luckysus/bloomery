import { useEffect, useState, type FormEvent } from "react";
import { Bot, Check, CircleHelp, Database, ExternalLink, HardDrive, KeyRound, Sparkles, Wrench } from "lucide-react";
import type { PermissionRuleRecord } from "../../bridge/desktop";
import { desktop } from "../../bridge/desktop";
import PermissionRulesPanel from "./PermissionRulesPanel";
import KnowledgeDatabasePanel from "./KnowledgeDatabasePanel";
import SettingsSkillsPanel from "./SettingsSkillsPanel";
import SettingsPreferencesPanel from "./SettingsPreferencesPanel";
import SettingsAgentPanel from "./SettingsAgentPanel";
import SettingsProvidersPanel from "./SettingsProvidersPanel";
import ModelRuntimeSettings from "./ModelRuntimeSettings";
import McpManagementPage from "../mcp/McpManagementPage";
import DatabaseConnectionsPanel from "./DatabaseConnectionsPanel";
import StorageDataPanel from "./StorageDataPanel";
import SettingsAccountPanel from "./SettingsAccountPanel";
import SettingsShortcutsPanel from "./SettingsShortcutsPanel";
import { SUNA_VERSION } from "../../version";
import type { ProviderSlot, RetrievalPlan, SettingsEditor } from "./settingsModel";
import type { MessageKey } from "../../i18n/locale";

export type SettingsPanelTab = "account" | "providers" | "general" | "appearance" | "knowledge" | "agent" | "mcp" | "skill" | "databases" | "shortcuts" | "about";
interface Props { activeTab: SettingsPanelTab; accountName: string; setAccountName: (value: string) => void; saveAccount: () => void; shortcutSend: string; saveShortcut: (value: string) => void; plan: RetrievalPlan; loading: boolean; editors: SettingsEditor[]; busySlot: ProviderSlot | null; testingSlot: ProviderSlot | null; updateEditor: (editor: SettingsEditor) => void; saveEditor: (event: FormEvent<HTMLFormElement>, editor: SettingsEditor) => void; testEditor: (editor: SettingsEditor) => void; deleteEditor: (editor: SettingsEditor) => void; changePlan: (plan: RetrievalPlan) => void; permissionRules: PermissionRuleRecord[]; permissionBusyId: string | null; revokePermission: (rule: PermissionRuleRecord) => void; t: (key: MessageKey, values?: Record<string, string | number>) => string; onOpenDiagnostics?: () => void; }

export default function SettingsPagePanel(props: Props) {
  const { activeTab, t } = props;
  if (activeTab === "account") return <SettingsAccountPanel />;
  if (activeTab === "general") return <><SettingsPreferencesPanel mode="general" /><div className="suna-settings-safety"><KeyRound size={18} aria-hidden="true" /><div><strong>{t("settingsSecretTitle")}</strong><span>{t("settingsSecretCopy")}</span></div></div></>;
  if (activeTab === "appearance") return <SettingsPreferencesPanel mode="appearance" />;
  if (activeTab === "knowledge") return <section className="suna-settings-category"><div className="suna-settings-section-intro"><span>KNOWLEDGE DATABASE</span><h2>{t("settingsCategoryKnowledge")}</h2><p>{t("settingsKnowledgeCopy")}</p></div><KnowledgeDatabasePanel /><SettingsProvidersPanel {...providerProps(props)} editors={props.editors.filter((editor) => editor.slot !== "chat")} /></section>;
  if (activeTab === "agent") return <><SettingsAgentPanel /><PermissionRulesPanel rules={props.permissionRules} busyId={props.permissionBusyId} onRevoke={props.revokePermission} /></>;
  if (activeTab === "mcp") return <McpManagementPage />;
  if (activeTab === "skill") return <SettingsSkillsPanel />;
  if (activeTab === "databases") return <><StorageDataPanel /><DatabaseConnectionsPanel /></>;
  if (activeTab === "shortcuts") return <SettingsShortcutsPanel />;
  if (activeTab === "about") return <AboutPanel onOpenDiagnostics={props.onOpenDiagnostics} />;
  return <><div className="suna-settings-section-intro"><span>MODEL ROUTING</span><h2>模型管理</h2><p>为聊天、Embedding、Reranker 和文档解析分别绑定 Provider。API Key 只写入系统凭据管理器。</p></div><div className="suna-model-capability-grid">{props.editors.map((editor) => <div className="suna-model-capability" key={editor.slot}><div><span className="suna-model-capability-icon">{editor.slot === "chat" ? <Bot size={16} /> : editor.slot === "embedding" ? <Sparkles size={16} /> : editor.slot === "reranker" ? <Check size={16} /> : <Database size={16} />}</span><strong>{editor.slot === "chat" ? "对话模型" : editor.slot === "embedding" ? "Embedding" : editor.slot === "reranker" ? "Reranker" : "文档解析"}</strong></div><span className={editor.id && editor.enabled ? "is-ready" : "is-missing"}>{editor.id && editor.enabled ? "已配置" : "待配置"}</span><small>{editor.modelId || editor.displayName}</small></div>)}</div><ModelRuntimeSettings /><SettingsProvidersPanel {...providerProps(props)} editors={props.editors} /></>;
}

function AboutPanel({ onOpenDiagnostics }: { onOpenDiagnostics?: () => void }) {
  const [storage, setStorage] = useState<"checking" | "ready" | "error" | "unavailable">("checking");
  const [knowledge, setKnowledge] = useState<"checking" | "ready" | "error" | "unavailable">("checking");
  useEffect(() => {
    let active = true;
    if (typeof desktop.getStorageHealth === "function") void desktop.getStorageHealth().then((value) => { if (active) setStorage(value.database_ok ? "ready" : "error"); }).catch(() => active && setStorage("error")); else setStorage("unavailable");
    if (typeof desktop.getKnowledgeDatabaseHealth === "function") void desktop.getKnowledgeDatabaseHealth().then((value) => { if (active) setKnowledge(value.connected ? "ready" : "error"); }).catch(() => active && setKnowledge("error")); else setKnowledge("unavailable");
    return () => { active = false; };
  }, []);
  const status = (value: string) => value === "ready" ? "正常" : value === "checking" ? "检查中" : value === "unavailable" ? "未连接" : "需要检查";
  return <section className="suna-settings-form-panel suna-about-panel" aria-labelledby="settings-about-heading"><header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">ABOUT SUNA</span><h2 id="settings-about-heading">关于 Suna</h2><p>钢铁材料研发智能体平台的版本、运行时和本地环境状态。</p></div><CircleHelp size={22} aria-hidden="true" /></header><div className="suna-about-brand"><span className="suna-about-mark">S</span><div><strong>Suna</strong><span>Steel Research Agent</span></div></div><div className="suna-settings-info-grid suna-about-health"><div><HardDrive size={16} /><span>SQLite 客户端</span><strong>{status(storage)}</strong></div><div><Database size={16} /><span>PostgreSQL 知识库</span><strong>{status(knowledge)}</strong></div><div><Bot size={16} /><span>Agent Runtime</span><strong>Rust</strong></div><div><Wrench size={16} /><span>桌面框架</span><strong>Tauri 2</strong></div></div><dl className="suna-settings-about"><div><dt>版本</dt><dd>{SUNA_VERSION}</dd></div><div><dt>前端</dt><dd>React 19 · Vite</dd></div><div><dt>许可</dt><dd>Apache-2.0</dd></div><div><dt>平台</dt><dd>Windows 10+</dd></div></dl><p className="suna-about-copy">Suna 将对话、Agent Loop、知识库、文献研究和本地工具统一在桌面端运行。</p><footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={() => window.open("https://github.com/luckysus/bloomery", "_blank")}><ExternalLink size={15} />项目仓库</button>{onOpenDiagnostics && <button type="button" className="suna-primary-button" onClick={onOpenDiagnostics}><Wrench size={15} />运行诊断</button>}</footer></section>;
}

function providerProps(props: Props) { return { plan: props.plan, loading: props.loading, busySlot: props.busySlot, testingSlot: props.testingSlot, onChange: props.updateEditor, onSubmit: props.saveEditor, onTest: (editor: SettingsEditor) => props.testEditor(editor), onDelete: (editor: SettingsEditor) => props.deleteEditor(editor), onPlanChange: props.changePlan }; }
