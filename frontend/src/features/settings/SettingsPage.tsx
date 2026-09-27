import { useEffect, useState, type FormEvent } from "react";
import {
  AlertCircle,
  Activity,
  Check,
  KeyRound,
  Settings2,
} from "lucide-react";
import { desktop, type PermissionRuleRecord, type ProviderCapability, type ProviderProfileInput } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import LanguageSelect from "../../components/common/LanguageSelect";
import PermissionRulesPanel from "./PermissionRulesPanel";
import DatabaseConnectionsPanel from "./DatabaseConnectionsPanel";
import KnowledgeDatabasePanel from "./KnowledgeDatabasePanel";
import SettingsSkillsPanel from "./SettingsSkillsPanel";
import McpServersPanel from "../extensions/McpServersPanel";
import { BLOOMERY_VERSION } from "../../version";
import SettingsPreferencesPanel from "./SettingsPreferencesPanel";
import SettingsAgentPanel from "./SettingsAgentPanel";
import SettingsProvidersPanel from "./SettingsProvidersPanel";
import SettingsTabList, { type SettingsTabOption } from "./SettingsTabList";
import {
  defaultRetrievalIds,
  defaults,
  editorFor,
  errorMessage,
  parseId,
  parseObject,
  profileForSlot,
  providerErrorMessage,
  type ProviderSlot,
  type RetrievalIds,
  type RetrievalPlan,
  type SettingsEditor,
} from "./settingsModel";
interface SettingsPageProps {
  onOpenDiagnostics?: () => void;
}
type SettingsTab =
  | "account" | "providers" | "general" | "appearance" | "knowledge"
  | "agent" | "mcp" | "skill" | "databases" | "shortcuts" | "about";
const settingsTabs: SettingsTabOption<SettingsTab>[] = [
  { id: "account", labelKey: "settingsCategoryAccount" },
  { id: "providers", labelKey: "settingsTabProviders" },
  { id: "general", labelKey: "settingsTabGeneral" },
  { id: "appearance", labelKey: "settingsCategoryAppearance" },
  { id: "knowledge", labelKey: "settingsCategoryKnowledge" },
  { id: "agent", labelKey: "settingsCategoryAgent" },
  { id: "mcp", labelKey: "settingsCategoryMcp" },
  { id: "skill", labelKey: "settingsCategorySkill" },
  { id: "databases", labelKey: "settingsTabDatabases" },
  { id: "shortcuts", labelKey: "settingsCategoryShortcuts" },
  { id: "about", labelKey: "settingsCategoryAbout" },
];

export default function SettingsPage({ onOpenDiagnostics }: SettingsPageProps) {
  const { t } = useLocale();
  const [activeTab, setActiveTab] = useState<SettingsTab>("providers");
  const [editors, setEditors] = useState<SettingsEditor[]>([]);
  const [plan, setPlan] = useState<RetrievalPlan>("free");
  const [retrievalIds, setRetrievalIds] = useState<RetrievalIds>(defaultRetrievalIds);
  const [completed, setCompleted] = useState<Record<string, unknown>>({});
  const [permissionRules, setPermissionRules] = useState<PermissionRuleRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busySlot, setBusySlot] = useState<ProviderSlot | null>(null);
  const [testingSlot, setTestingSlot] = useState<ProviderSlot | null>(null);
  const [permissionBusyId, setPermissionBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [accountName, setAccountName] = useState("Bloomery");
  const [shortcutSend, setShortcutSend] = useState("Ctrl+Enter");
  const [query, setQuery] = useState("");
  const [fileInputKey, setFileInputKey] = useState(0);
  const visibleTabs = settingsTabs.filter((tab) => {
    if (!query.trim()) return true;
    return t(tab.labelKey).toLowerCase().includes(query.trim().toLowerCase());
  });
  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const [profiles, completedValue, retrievalValue, permissions] = await Promise.all([
        desktop.listProviderProfiles(),
        desktop.getSetting("onboarding.completed"),
        desktop.getSetting("onboarding.retrieval"),
        desktop.listPermissionRules(),
      ]);
      const completed = parseObject(completedValue);
      const retrieval = parseObject(retrievalValue);
      const nextIds: RetrievalIds = {
        embedding: parseId(retrieval.embedding_profile_id),
        reranker: parseId(retrieval.reranker_profile_id),
        mineru: parseId(retrieval.mineru_profile_id),
      };
      setCompleted(completed);
      setRetrievalIds(nextIds);
      setPermissionRules(permissions);
      setPlan(retrieval.plan === "pro" ? "pro" : "free");
      setEditors((Object.keys(defaults) as ProviderSlot[]).map((slot) =>
        editorFor(slot, profileForSlot(slot, profiles, completed, retrieval)),
      ));
      const accountValue = parseObject(await desktop.getSetting("profile.account"));
      const shortcutValue = parseObject(await desktop.getSetting("ui.shortcuts"));
      setAccountName(typeof accountValue.display_name === "string" && accountValue.display_name.trim() ? accountValue.display_name : "Bloomery");
      setShortcutSend(typeof shortcutValue.send === "string" && shortcutValue.send ? shortcutValue.send : "Ctrl+Enter");
    } catch (cause) {
      setError(errorMessage(cause, t("settingsLoadError")));
    } finally {
      setLoading(false);
    }
  };

  const saveAccount = async () => {
    await desktop.setSetting("profile.account", JSON.stringify({ version: 1, display_name: accountName.trim() || "Bloomery" }));
    setNotice(t("settingsSaved"));
  };

  const saveShortcut = async (value: string) => {
    setShortcutSend(value);
    await desktop.setSetting("ui.shortcuts", JSON.stringify({ version: 1, send: value }));
    setNotice(t("settingsSaved"));
  };
  const resetPreferences = async () => {
    if (!window.confirm(t("settingsResetConfirm"))) return;
    await Promise.all([
      desktop.setSetting("ui.preferences", JSON.stringify({})),
      desktop.setSetting("agent.preferences", JSON.stringify({})),
      desktop.setSetting("ui.shortcuts", JSON.stringify({})),
      desktop.setSetting("profile.account", JSON.stringify({ display_name: "Bloomery" })),
    ]);
    setAccountName("Bloomery");
    setShortcutSend("Ctrl+Enter");
    setNotice(t("settingsResetDone"));
  };
  const exportSettings = () => {
    const payload = { version: 1, exported_at: new Date().toISOString(), settings: { account: accountName, shortcutSend } };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a"); anchor.href = url; anchor.download = "bloomery-settings.json"; anchor.click(); URL.revokeObjectURL(url);
    setNotice(t("settingsExported"));
  };
  const importSettings = async (file: File) => {
    try {
      const parsed = JSON.parse(await file.text()) as { settings?: { account?: string; shortcutSend?: string } };
      if (typeof parsed.settings?.account === "string") { setAccountName(parsed.settings.account); await desktop.setSetting("profile.account", JSON.stringify({ display_name: parsed.settings.account })); }
      if (parsed.settings?.shortcutSend === "Ctrl+Enter" || parsed.settings?.shortcutSend === "Enter") { setShortcutSend(parsed.settings.shortcutSend); await desktop.setSetting("ui.shortcuts", JSON.stringify({ send: parsed.settings.shortcutSend })); }
      setNotice(t("settingsImported"));
    } catch (cause) { setError(errorMessage(cause, t("settingsImportError"))); }
    setFileInputKey((key) => key + 1);
  };
  useEffect(() => {
    void load();
  }, []);
  const persistRetrieval = async (nextPlan: RetrievalPlan, ids: RetrievalIds) => {
    await desktop.setSetting("onboarding.retrieval", JSON.stringify({
      version: 1,
      state: "configured",
      plan: nextPlan,
      embedding_profile_id: ids.embedding,
      reranker_profile_id: ids.reranker,
      mineru_profile_id: ids.mineru,
    }));
  };
  const updateEditor = (next: SettingsEditor) => {
    setEditors((current) => current.map((editor) => editor.slot === next.slot ? next : editor));
  };
  const capabilityForSlot: Record<ProviderSlot, ProviderCapability> = {
    chat: "chat",
    embedding: "embedding",
    reranker: "rerank",
    mineru: "document_parser",
  };
  const changePlan = async (nextPlan: RetrievalPlan) => {
    const previousPlan = plan;
    setPlan(nextPlan);
    setError(null);
    try {
      await persistRetrieval(nextPlan, retrievalIds);
      setNotice(t("settingsSaved"));
    } catch (cause) {
      setPlan(previousPlan);
      setError(errorMessage(cause, t("settingsSaveError")));
    }
  };
  const saveEditor = async (event: FormEvent<HTMLFormElement>, editor: SettingsEditor) => {
    event.preventDefault();
    setBusySlot(editor.slot);
    setError(null);
    setNotice(null);
    const credentialName = editor.kind === "ollama" ? null : "api_key";
    try {
      const input: ProviderProfileInput = {
        id: editor.id ?? undefined,
        kind: editor.kind,
        display_name: editor.displayName,
        base_url: editor.baseUrl,
        model_id: editor.modelId || null,
        credential_name: credentialName,
        enabled: editor.enabled,
      };
      const saved = await desktop.saveProviderProfile(input);
      if (credentialName && editor.apiKey.trim()) {
        await desktop.setProviderSecret(saved.id, credentialName, editor.apiKey.trim());
      }
      await desktop.setDefaultProvider(
        capabilityForSlot[editor.slot],
        editor.enabled ? saved.id : null,
      );
      if (editor.slot === "chat") {
        const nextCompleted = {
          ...completed,
          version: typeof completed.version === "number" ? completed.version : 1,
          completed: true,
          llm_profile_id: editor.enabled ? saved.id : null,
        };
        await desktop.setSetting("onboarding.completed", JSON.stringify(nextCompleted));
        setCompleted(nextCompleted);
      } else {
        const nextIds = { ...retrievalIds, [editor.slot]: saved.id } as RetrievalIds;
        setRetrievalIds(nextIds);
        await persistRetrieval(plan, nextIds);
      }
      updateEditor({
        ...editor,
        id: saved.id,
        kind: saved.kind,
        displayName: saved.display_name,
        baseUrl: saved.base_url,
        modelId: saved.model_id ?? "",
        apiKey: "",
        enabled: saved.enabled,
        secretConfigured: saved.secret_configured || Boolean(editor.apiKey.trim()),
      });
      setNotice(t("settingsSaved"));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsSaveError")));
    } finally {
      setBusySlot(null);
    }
  };

  const testEditor = async (editor: SettingsEditor) => {
    if (!editor.id) {
      setError(t("settingsSaveBeforeTest"));
      return;
    }
    setTestingSlot(editor.slot);
    setError(null);
    setNotice(null);
    try {
      const result = await desktop.testProviderProfile(
        editor.id,
        capabilityForSlot[editor.slot],
      );
      if (!result.ok) throw new Error(providerErrorMessage(result.error_code, (key) => t(key)));
      setNotice(t("settingsTestPassed"));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsTestFailed")));
    } finally {
      setTestingSlot(null);
    }
  };

  const deleteEditor = async (editor: SettingsEditor) => {
    if (!editor.id || !window.confirm(t("settingsDeleteConfirm", { name: editor.displayName }))) return;
    setBusySlot(editor.slot);
    setError(null);
    try {
      await desktop.deleteProviderProfile(editor.id);
      await desktop.setDefaultProvider(capabilityForSlot[editor.slot], null);
      updateEditor(editorFor(editor.slot, undefined));
      const nextIds = editor.slot === "chat" ? retrievalIds : { ...retrievalIds, [editor.slot]: null } as RetrievalIds;
      setRetrievalIds(nextIds);
      if (editor.slot !== "chat") await persistRetrieval(plan, nextIds);
      setNotice(t("settingsDeleted"));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsDeleteError")));
    } finally {
      setBusySlot(null);
    }
  };

  const revokePermission = async (rule: PermissionRuleRecord) => {
    setPermissionBusyId(rule.id);
    setError(null);
    setNotice(null);
    try {
      await desktop.revokePermissionRule(rule.id);
      setPermissionRules((current) => current.filter((candidate) => candidate.id !== rule.id));
      setNotice(t("settingsDeleted"));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsSaveError")));
    } finally {
      setPermissionBusyId(null);
    }
  };

  return (
    <section className="bloomery-settings bloomery-page-surface" aria-labelledby="settings-heading">
      <header className="bloomery-settings-header">
        <div>
          <h1 id="settings-heading">{t("settingsTitle")}</h1>
        </div>
        <div className="bloomery-settings-header-actions">
          <LanguageSelect />
          {onOpenDiagnostics && (
            <button
              type="button"
              className="bloomery-action-secondary bloomery-settings-diagnostics-button"
              onClick={onOpenDiagnostics}
            >
              <Activity size={16} aria-hidden="true" />
              {t("settingsDiagnostics")}
            </button>
          )}
          <button type="button" className="bloomery-icon-button" onClick={() => void load()} disabled={loading} aria-label={t("settingsRefresh")} title={t("settingsRefresh")}>
            <Settings2 size={18} aria-hidden="true" />
          </button>
        </div>
      </header>

      {error && <div className="bloomery-settings-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-settings-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}
      <div className="bloomery-settings-toolbar">
        <input type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("settingsSearchPlaceholder")} aria-label={t("settingsSearch")} />
        <button type="button" className="bloomery-action-secondary" onClick={exportSettings}>{t("settingsExport")}</button>
        <label className="bloomery-action-secondary">{t("settingsImport")}<input key={fileInputKey} type="file" accept="application/json" hidden onChange={(event) => { const file = event.target.files?.[0]; if (file) void importSettings(file); }} /></label>
        <button type="button" className="bloomery-action-secondary" onClick={() => void resetPreferences()}>{t("settingsReset")}</button>
      </div>

      <div className="bloomery-settings-layout">
        <SettingsTabList tabs={visibleTabs} activeTab={activeTab} onSelect={setActiveTab} />

        <div role="tabpanel" id={`settings-panel-${activeTab}`} aria-labelledby={`settings-tab-${activeTab}`}>
        {activeTab === "account" && (
          <section className="bloomery-settings-category">
            <h2>{t("settingsCategoryAccount")}</h2>
            <p>{t("settingsAccountCopy")}</p>
            <div className="bloomery-account-summary"><span className="bloomery-account-avatar">B</span><div><strong>{accountName}</strong><span>{t("localAccount")}</span></div></div>
            <label className="bloomery-settings-field">{t("settingsAccountName")}<input value={accountName} onChange={(event) => setAccountName(event.target.value)} /></label>
            <button type="button" className="bloomery-action-primary" onClick={() => void saveAccount()}>{t("settingsSave")}</button>
          </section>
        )}
        {activeTab === "general" && (
          <><SettingsPreferencesPanel mode="general" /><div className="bloomery-settings-safety"><KeyRound size={18} aria-hidden="true" /><div><strong>{t("settingsSecretTitle")}</strong><span>{t("settingsSecretCopy")}</span></div></div></>
        )}
        {activeTab === "appearance" && <SettingsPreferencesPanel mode="appearance" />}
        {activeTab === "knowledge" && <section className="bloomery-settings-category"><h2>{t("settingsCategoryKnowledge")}</h2><p>{t("settingsKnowledgeCopy")}</p><KnowledgeDatabasePanel /><SettingsProvidersPanel plan={plan} loading={loading} editors={editors.filter((editor) => editor.slot !== "chat")} busySlot={busySlot} testingSlot={testingSlot} onChange={updateEditor} onSubmit={saveEditor} onTest={(editor) => void testEditor(editor)} onDelete={(editor) => void deleteEditor(editor)} onPlanChange={(nextPlan) => void changePlan(nextPlan)} /></section>}
        {activeTab === "agent" && <><SettingsAgentPanel /><PermissionRulesPanel rules={permissionRules} busyId={permissionBusyId} onRevoke={(rule) => void revokePermission(rule)} /></>}
        {activeTab === "mcp" && <McpServersPanel />}
        {activeTab === "skill" && <SettingsSkillsPanel />}
        {activeTab === "databases" && <DatabaseConnectionsPanel />}
        {activeTab === "providers" && (
          <SettingsProvidersPanel
            plan={plan}
            loading={loading}
            editors={editors}
            busySlot={busySlot}
            testingSlot={testingSlot}
            onChange={updateEditor}
            onSubmit={saveEditor}
            onTest={(editor) => void testEditor(editor)}
            onDelete={(editor) => void deleteEditor(editor)}
            onPlanChange={(nextPlan) => void changePlan(nextPlan)}
          />
        )}
        {activeTab === "shortcuts" && <section className="bloomery-settings-category"><h2>{t("settingsCategoryShortcuts")}</h2><p>{t("settingsShortcutsCopy")}</p><label className="bloomery-settings-field">{t("settingsShortcutSend")}<select value={shortcutSend} onChange={(event) => void saveShortcut(event.target.value)}><option>Ctrl+Enter</option><option>Enter</option></select></label></section>}
        {activeTab === "about" && <section className="bloomery-settings-category"><h2>{t("settingsCategoryAbout")}</h2><p>{t("settingsAboutCopy")}</p><dl className="bloomery-settings-about"><div><dt>{t("settingsVersion")}</dt><dd>{BLOOMERY_VERSION}</dd></div><div><dt>{t("settingsRuntime")}</dt><dd>Tauri Desktop</dd></div></dl></section>}
        </div>
      </div>
    </section>
  );
}
