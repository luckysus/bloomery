import { useEffect, useState, type FormEvent } from "react";
import {
  Bot,
  CircleHelp,
  Database,
  HardDrive,
  Keyboard,
  Palette,
  Server,
  Settings2,
  Sparkles,
  UserRound,
} from "lucide-react";
import { desktop, type PermissionRuleRecord, type ProviderCapability, type ProviderProfileInput } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import SettingsTabList, { type SettingsTabOption } from "./SettingsTabList";
import SettingsPagePanel from "./SettingsPagePanel";
import SettingsPageChrome from "./SettingsPageChrome";
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
import "./settings-v2.css";
interface SettingsPageProps { onOpenDiagnostics?: () => void; initialTab?: SettingsTab; }
type SettingsTab = "account" | "providers" | "general" | "appearance" | "knowledge" | "agent" | "mcp" | "skill" | "databases" | "shortcuts" | "about";
const settingsTabs: SettingsTabOption<SettingsTab>[] = [
  { id: "general", labelKey: "settingsTabGeneral", icon: Settings2 },
  { id: "appearance", labelKey: "settingsCategoryAppearance", icon: Palette },
  { id: "providers", labelKey: "settingsTabProviders", icon: Sparkles },
  { id: "knowledge", labelKey: "settingsCategoryKnowledge", icon: Database },
  { id: "agent", labelKey: "settingsCategoryAgent", icon: Bot },
  { id: "databases", labelKey: "settingsTabDatabases", icon: HardDrive },
  { id: "about", labelKey: "settingsCategoryAbout", icon: CircleHelp },
  { id: "account", labelKey: "settingsCategoryAccount", icon: UserRound },
  { id: "mcp", labelKey: "settingsCategoryMcp", icon: Server },
  { id: "skill", labelKey: "settingsCategorySkill", icon: CircleHelp },
  { id: "shortcuts", labelKey: "settingsCategoryShortcuts", icon: Keyboard },
];export default function SettingsPage({ onOpenDiagnostics, initialTab }: SettingsPageProps) {
  const { t } = useLocale();
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab ?? "appearance");
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
  const [accountName, setAccountName] = useState("Suna");
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
      setAccountName(typeof accountValue.display_name === "string" && accountValue.display_name.trim() ? accountValue.display_name : "Suna");
      setShortcutSend(typeof shortcutValue.send === "string" && shortcutValue.send ? shortcutValue.send : "Ctrl+Enter");
    } catch (cause) {
      setError(errorMessage(cause, t("settingsLoadError")));
    } finally {
      setLoading(false);
    }
  };
  const saveAccount = async () => {
    await desktop.setSetting("profile.account", JSON.stringify({ version: 1, display_name: accountName.trim() || "Suna" }));
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
      desktop.setSetting("profile.account", JSON.stringify({ display_name: "Suna" })),
    ]);
    setAccountName("Suna");
    setShortcutSend("Ctrl+Enter");
    setNotice(t("settingsResetDone"));
  };
  const exportSettings = () => {
    const payload = { version: 1, exported_at: new Date().toISOString(), settings: { account: accountName, shortcutSend } };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a"); anchor.href = url; anchor.download = "suna-settings.json"; anchor.click(); URL.revokeObjectURL(url);
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
  useEffect(() => {
    if (initialTab) setActiveTab(initialTab);
  }, [initialTab]);
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
    <section className="suna-settings suna-settings-v2 suna-page-surface" aria-labelledby="settings-heading">
      <SettingsPageChrome t={t} loading={loading} error={error} notice={notice} query={query} setQuery={setQuery} load={() => void load()} exportSettings={exportSettings} importSettings={(file) => void importSettings(file)} fileInputKey={fileInputKey} resetPreferences={() => void resetPreferences()} onOpenDiagnostics={onOpenDiagnostics} showUtilities={activeTab !== "appearance"} />
      <div className={`suna-settings-layout ${activeTab === "appearance" ? "is-appearance" : ""}`}>
        <SettingsTabList tabs={visibleTabs} activeTab={activeTab} onSelect={setActiveTab} />
        <div role="tabpanel" id={`settings-panel-${activeTab}`} aria-labelledby={`settings-tab-${activeTab}`}>
          <SettingsPagePanel activeTab={activeTab} accountName={accountName} setAccountName={setAccountName} saveAccount={() => void saveAccount()} shortcutSend={shortcutSend} saveShortcut={(value) => void saveShortcut(value)} plan={plan} loading={loading} editors={editors} busySlot={busySlot} testingSlot={testingSlot} updateEditor={updateEditor} saveEditor={saveEditor} testEditor={(editor) => void testEditor(editor)} deleteEditor={(editor) => void deleteEditor(editor)} changePlan={(nextPlan) => void changePlan(nextPlan)} permissionRules={permissionRules} permissionBusyId={permissionBusyId} revokePermission={(rule) => void revokePermission(rule)} t={t} onOpenDiagnostics={onOpenDiagnostics} />
        </div>
      </div>
    </section>
  );
}
