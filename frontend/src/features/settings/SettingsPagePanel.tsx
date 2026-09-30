import type { FormEvent } from "react";
import { Bot, Check, Database, KeyRound, Sparkles } from "lucide-react";
import type { PermissionRuleRecord } from "../../bridge/desktop";
import PermissionRulesPanel from "./PermissionRulesPanel";
import KnowledgeDatabasePanel from "./KnowledgeDatabasePanel";
import SettingsSkillsPanel from "./SettingsSkillsPanel";
import SettingsPreferencesPanel from "./SettingsPreferencesPanel";
import SettingsAgentPanel from "./SettingsAgentPanel";
import SettingsProvidersPanel from "./SettingsProvidersPanel";
import ModelRuntimeSettings from "./ModelRuntimeSettings";
import McpManagementPage from "../mcp/McpManagementPage";
import DatabaseConnectionsPanel from "./DatabaseConnectionsPanel";
import { SUNA_VERSION } from "../../version";
import type { ProviderSlot, RetrievalPlan, SettingsEditor } from "./settingsModel";
import type { MessageKey } from "../../i18n/locale";

export type SettingsPanelTab = "account" | "providers" | "general" | "appearance" | "knowledge" | "agent" | "mcp" | "skill" | "databases" | "shortcuts" | "about";

interface Props {
  activeTab: SettingsPanelTab;
  accountName: string;
  setAccountName: (value: string) => void;
  saveAccount: () => void;
  shortcutSend: string;
  saveShortcut: (value: string) => void;
  plan: RetrievalPlan;
  loading: boolean;
  editors: SettingsEditor[];
  busySlot: ProviderSlot | null;
  testingSlot: ProviderSlot | null;
  updateEditor: (editor: SettingsEditor) => void;
  saveEditor: (event: FormEvent<HTMLFormElement>, editor: SettingsEditor) => void;
  testEditor: (editor: SettingsEditor) => void;
  deleteEditor: (editor: SettingsEditor) => void;
  changePlan: (plan: RetrievalPlan) => void;
  permissionRules: PermissionRuleRecord[];
  permissionBusyId: string | null;
  revokePermission: (rule: PermissionRuleRecord) => void;
  t: (key: MessageKey, values?: Record<string, string | number>) => string;
  onOpenDiagnostics?: () => void;
}

export default function SettingsPagePanel(props: Props) {
  const { activeTab, t } = props;
  if (activeTab === "account") return <section className="suna-settings-category"><h2>{t("settingsCategoryAccount")}</h2><p>{t("settingsAccountCopy")}</p><div className="suna-account-summary"><span className="suna-account-avatar">S</span><div><strong>{props.accountName}</strong><span>{t("localAccount")}</span></div></div><label className="suna-settings-field">{t("settingsAccountName")}<input value={props.accountName} onChange={(event) => props.setAccountName(event.target.value)} /></label><button type="button" className="suna-action-primary" onClick={props.saveAccount}>{t("settingsSave")}</button></section>;
  if (activeTab === "general") return <><SettingsPreferencesPanel mode="general" /><div className="suna-settings-safety"><KeyRound size={18} aria-hidden="true" /><div><strong>{t("settingsSecretTitle")}</strong><span>{t("settingsSecretCopy")}</span></div></div></>;
  if (activeTab === "appearance") return <SettingsPreferencesPanel mode="appearance" />;
  if (activeTab === "knowledge") return <section className="suna-settings-category"><h2>{t("settingsCategoryKnowledge")}</h2><p>{t("settingsKnowledgeCopy")}</p><KnowledgeDatabasePanel /><SettingsProvidersPanel {...providerProps(props)} editors={props.editors.filter((editor) => editor.slot !== "chat")} /></section>;
  if (activeTab === "agent") return <><SettingsAgentPanel /><PermissionRulesPanel rules={props.permissionRules} busyId={props.permissionBusyId} onRevoke={props.revokePermission} /></>;
  if (activeTab === "mcp") return <McpManagementPage />;
  if (activeTab === "skill") return <SettingsSkillsPanel />;
  if (activeTab === "databases") return <DatabaseConnectionsPanel />;
  if (activeTab === "shortcuts") return <section className="suna-settings-category"><h2>{t("settingsCategoryShortcuts")}</h2><p>{t("settingsShortcutsCopy")}</p><label className="suna-settings-field">{t("settingsShortcutSend")}<select value={props.shortcutSend} onChange={(event) => props.saveShortcut(event.target.value)}><option>Ctrl+Enter</option><option>Enter</option></select></label></section>;
  if (activeTab === "about") return <section className="suna-settings-category suna-about-panel"><div className="suna-about-brand"><span className="suna-about-mark">S</span><div><h2>Suna</h2><p>钢铁材料研发智能体平台</p></div></div><p>{t("settingsAboutCopy")}</p><dl className="suna-settings-about"><div><dt>{t("settingsVersion")}</dt><dd>{SUNA_VERSION}</dd></div><div><dt>{t("settingsRuntime")}</dt><dd>Tauri Desktop</dd></div><div><dt>前端</dt><dd>React 19 · Vite</dd></div><div><dt>核心运行时</dt><dd>Rust Agent Runtime</dd></div></dl><div className="suna-about-links"><button type="button" onClick={() => window.open("https://github.com/openvetta/open-vetta", "_blank")}>项目仓库</button><button type="button" onClick={props.onOpenDiagnostics}>运行诊断</button></div></section>;
  return <><div className="suna-settings-section-intro"><span>MODEL ROUTING</span><h2>模型配置</h2><p>为聊天、Embedding、Reranker 和文档解析分别绑定 Provider。API Key 只写入系统凭据管理器。</p></div><div className="suna-model-capability-grid">{props.editors.map((editor) => <div className="suna-model-capability" key={editor.slot}><div><span className="suna-model-capability-icon">{editor.slot === "chat" ? <Bot size={16} /> : editor.slot === "embedding" ? <Sparkles size={16} /> : editor.slot === "reranker" ? <Check size={16} /> : <Database size={16} />}</span><strong>{editor.slot === "chat" ? "对话模型" : editor.slot === "embedding" ? "Embedding" : editor.slot === "reranker" ? "Reranker" : "文档解析"}</strong></div><span className={editor.id && editor.enabled ? "is-ready" : "is-missing"}>{editor.id && editor.enabled ? "已配置" : "待配置"}</span><small>{editor.modelId || editor.displayName}</small></div>)}</div><ModelRuntimeSettings /><SettingsProvidersPanel {...providerProps(props)} editors={props.editors} /></>;
}

function providerProps(props: Props) {
  return { plan: props.plan, loading: props.loading, busySlot: props.busySlot, testingSlot: props.testingSlot, onChange: props.updateEditor, onSubmit: props.saveEditor, onTest: (editor: SettingsEditor) => props.testEditor(editor), onDelete: (editor: SettingsEditor) => props.deleteEditor(editor), onPlanChange: props.changePlan };
}
