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
import { desktop, isDesktopRuntime, type AgentProfileSummary, type PermissionRuleRecord, type ProviderCapability, type ProviderProfileInput } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import SettingsTabList, { type SettingsTabOption } from "./SettingsTabList";
import SettingsPagePanel from "./SettingsPagePanel";
import SettingsPageChrome from "./SettingsPageChrome";
import {
  defaultRetrievalIds,
  defaults,
  editorFor,
  getSettingValue,
  parseId,
  parseObject,
  profileForSlot,
  providerErrorMessage,
  setSettingValue,
  type ProviderSlot,
  type ProviderRuntimeMeta,
  type RetrievalIds,
  type RetrievalPlan,
  type SettingsEditor,
} from "./settingsModel";
import { settingsErrorMessage } from "./settingsError";
import { normalizeAgentPreferences } from "./SettingsAgentPanel";
import "./settings.css";
interface SettingsPageProps { onOpenDiagnostics?: () => void; initialTab?: SettingsTab; }
export type SettingsTab = "account" | "providers" | "general" | "appearance" | "knowledge" | "agent" | "mcp" | "skill" | "databases" | "shortcuts" | "about";
const localSettingKey = (key: string) => `suna.setting.${key}`;
const readLocalSetting = (key: string) => {
  try {
    return window.localStorage.getItem(localSettingKey(key));
  } catch {
    return null;
  }
};
const writeLocalSetting = (key: string, value: string) => {
  window.localStorage.setItem(localSettingKey(key), value);
  if (key === "ui.theme") window.localStorage.setItem("suna.ui.theme", value);
  if (key === "ui.locale") window.localStorage.setItem("suna.ui.locale", value);
  if (key === "ui.preferences") window.localStorage.setItem("suna.ui.preferences", value);
};
const settingsTabs: SettingsTabOption<SettingsTab>[] = [
  { id: "account", labelKey: "settingsCategoryAccount", icon: UserRound },
  { id: "providers", labelKey: "settingsTabProviders", icon: Sparkles },
  { id: "general", labelKey: "settingsTabGeneral", icon: Settings2 },
  { id: "appearance", labelKey: "settingsCategoryAppearance", icon: Palette },
  { id: "knowledge", labelKey: "settingsCategoryKnowledge", icon: Database },
  { id: "agent", labelKey: "settingsCategoryAgent", icon: Bot },
  { id: "mcp", labelKey: "settingsCategoryMcp", icon: Server },
  { id: "skill", labelKey: "settingsCategorySkill", icon: CircleHelp },
  { id: "databases", labelKey: "settingsTabDatabases", icon: HardDrive },
  { id: "shortcuts", labelKey: "settingsCategoryShortcuts", icon: Keyboard },
  { id: "about", labelKey: "settingsCategoryAbout", icon: CircleHelp },
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
  const [settingsSnapshot, setSettingsSnapshot] = useState<Record<string, string | null>>({});
  const visibleTabs = settingsTabs.filter((tab) => {
    if (!query.trim()) return true;
    return t(tab.labelKey).toLowerCase().includes(query.trim().toLowerCase());
  });
  const load = async () => {
    setLoading(true);
    setError(null);
    // The Vite browser preview has no Tauri command bridge. Keep the UI
    // usable with safe defaults there; a real desktop command failure still
    // follows the error path below.
    if (!isDesktopRuntime()) {
      setCompleted({});
      setRetrievalIds(defaultRetrievalIds);
      setPermissionRules([]);
      setPlan("free");
      setEditors((Object.keys(defaults) as ProviderSlot[]).map((slot) => editorFor(slot, undefined)));
      const account = parseObject(readLocalSetting("profile.account"));
      const shortcut = parseObject(readLocalSetting("ui.shortcuts"));
      setAccountName(typeof account.display_name === "string" && account.display_name.trim() ? account.display_name : "Suna");
      setShortcutSend(typeof shortcut.send === "string" && shortcut.send ? shortcut.send : "Ctrl+Enter");
      setSettingsSnapshot(Object.fromEntries(["profile.account", "ui.shortcuts", "ui.settings_tab", "ui.theme", "ui.locale", "ui.preferences", "agent.preferences", "model.preferences", "model.provider.preferences", "knowledge.preferences"].map((key) => [key, readLocalSetting(key)])));
      if (!initialTab && typeof window !== "undefined") {
        const savedTab = window.sessionStorage.getItem("suna.settings.tab")
          || window.localStorage.getItem("suna.settings.tab")
          || parseObject(readLocalSetting("ui.settings_tab")).tab;
        if (typeof savedTab === "string" && settingsTabs.some((tab) => tab.id === savedTab)) setActiveTab(savedTab as SettingsTab);
      }
      setLoading(false);
      return;
    }
    try {
      const [profiles, completedValue, retrievalValue, permissions, accountValue, shortcutValue, tabValue, themeValue, localeValue, appearanceValue, agentValue, modelValue, providerMetaValue, knowledgeValue] = await Promise.all([
        desktop.listProviderProfiles(),
        desktop.getSetting("onboarding.completed"),
        desktop.getSetting("onboarding.retrieval"),
        desktop.listPermissionRules(),
        desktop.getSetting("profile.account"),
        desktop.getSetting("ui.shortcuts"),
        desktop.getSetting("ui.settings_tab"),
        desktop.getSetting("ui.theme"),
        desktop.getSetting("ui.locale"),
        desktop.getSetting("ui.preferences"),
        desktop.getSetting("agent.preferences"),
        desktop.getSetting("model.preferences"),
        desktop.getSetting("model.provider.preferences"),
        desktop.getSetting("knowledge.preferences"),
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
      const providerMeta = parseObject(providerMetaValue);
      setEditors((Object.keys(defaults) as ProviderSlot[]).map((slot) => {
        const profile = profileForSlot(slot, profiles, completed, retrieval);
        const rawMeta = profile?.id ? providerMeta[profile.id] : undefined;
        const meta: ProviderRuntimeMeta = rawMeta && typeof rawMeta === "object" && !Array.isArray(rawMeta)
          ? rawMeta as ProviderRuntimeMeta
          : {};
        return editorFor(slot, profile, meta);
      }));
      const account = parseObject(accountValue);
      const shortcut = parseObject(shortcutValue);
      setAccountName(typeof account.display_name === "string" && account.display_name.trim() ? account.display_name : "Suna");
      setShortcutSend(typeof shortcut.send === "string" && shortcut.send ? shortcut.send : "Ctrl+Enter");
      setSettingsSnapshot({
        "profile.account": accountValue,
        "ui.shortcuts": shortcutValue,
        "ui.settings_tab": tabValue,
        "ui.theme": themeValue,
        "ui.locale": localeValue,
        "ui.preferences": appearanceValue,
        "agent.preferences": agentValue,
        "model.preferences": modelValue,
        "model.provider.preferences": providerMetaValue,
        "knowledge.preferences": knowledgeValue,
      });
      if (!initialTab) {
        const sessionTab = typeof window !== "undefined"
          ? window.sessionStorage.getItem("suna.settings.tab") ?? window.localStorage.getItem("suna.settings.tab")
          : null;
        const savedTab = sessionTab || parseObject(tabValue).tab;
        if (typeof savedTab === "string" && settingsTabs.some((tab) => tab.id === savedTab)) setActiveTab(savedTab as SettingsTab);
      }
    } catch (cause) {
      setError(settingsErrorMessage(cause, t("settingsLoadError")));
    } finally {
      setLoading(false);
    }
  };
  const persistSetting = (key: string, value: string) => setSettingValue(key, value);
  const saveAccount = async () => {
    await persistSetting("profile.account", JSON.stringify({ version: 1, display_name: accountName.trim() || "Suna" }));
    setNotice(t("settingsSaved"));
  };
  const saveShortcut = async (value: string) => {
    setShortcutSend(value);
    await persistSetting("ui.shortcuts", JSON.stringify({ version: 1, send: value }));
    setNotice(t("settingsSaved"));
  };
  const resetPreferences = async () => {
    if (!window.confirm(t("settingsResetConfirm"))) return;
    const resetValues: Record<string, string> = {
      "ui.preferences": JSON.stringify({}),
      "ui.theme": JSON.stringify({ version: 1, preference: "light" }),
      "ui.locale": JSON.stringify({ version: 1, preference: "zh-CN" }),
      "ui.settings_tab": JSON.stringify({ version: 1, tab: "appearance" }),
      "agent.preferences": JSON.stringify({}),
      "model.preferences": JSON.stringify({}),
      "model.provider.preferences": JSON.stringify({}),
      "knowledge.preferences": JSON.stringify({}),
      "ui.shortcuts": JSON.stringify({}),
      "profile.account": JSON.stringify({ display_name: "Suna" }),
    };
    try {
      await Promise.all(Object.entries(resetValues).map(([key, value]) => persistSetting(key, value)));
      setAccountName("Suna");
      setShortcutSend("Ctrl+Enter");
      setActiveTab("appearance");
      setSettingsSnapshot(resetValues);
      if (typeof window !== "undefined") {
        window.sessionStorage.setItem("suna.settings.tab", "appearance");
        window.dispatchEvent(new Event("suna:settings-reset"));
      }
      setNotice(t("settingsResetDone"));
    } catch (cause) {
      setError(settingsErrorMessage(cause, t("settingsResetError")));
    }
  };
  const exportSettings = () => {
    void (async () => {
      const keys = ["profile.account", "ui.shortcuts", "ui.settings_tab", "ui.theme", "ui.locale", "ui.preferences", "agent.preferences", "model.preferences", "model.provider.preferences", "knowledge.preferences", "knowledge.postgres", "onboarding.completed", "onboarding.retrieval"];
      const liveValues: Record<string, string | null> = {};
      if (isDesktopRuntime()) {
        const values = await Promise.all(keys.map(async (key) => [key, await desktop.getSetting(key)] as const));
        for (const [key, value] of values) liveValues[key] = value;
      }
      const extensions: Record<string, unknown> = {};
      if (isDesktopRuntime()) {
        const [mcpServers, skillCatalog, agentProfiles] = await Promise.all([
          desktop.listMcpServers().catch(() => []),
          desktop.listSkills().catch(() => ({ skills: [], errors: [] })),
          desktop.listAgentProfiles(),
        ]);
        // Export MCP metadata only. Credentials, environment values and bearer
        // tokens stay in the local credential store and never enter this file.
        extensions.mcp_servers = mcpServers.map(({ id, display_name, server_id, transport, url, executable, args, working_directory, inherited_env, timeout_ms, enabled }) => ({ id, display_name, server_id, transport, url, executable, args, working_directory, inherited_env, timeout_ms, enabled }));
        extensions.skills = skillCatalog.skills.map(({ name, enabled }) => ({ name, enabled }));
        extensions.agent_profiles = agentProfiles;
      }
      const payload = {
        version: 2,
        exported_at: new Date().toISOString(),
        settings: {
          ...settingsSnapshot,
          ...liveValues,
        },
        extensions,
      };
      const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a"); anchor.href = url; anchor.download = "suna-settings.json"; anchor.click(); URL.revokeObjectURL(url);
      setNotice(t("settingsExported"));
    })().catch((cause) => setError(settingsErrorMessage(cause, t("settingsExportError"))));
  };
  const importSettings = async (file: File) => {
    try {
      const parsed = JSON.parse(await file.text()) as { settings?: Record<string, unknown> & { account?: string; shortcutSend?: string }; extensions?: { mcp_servers?: unknown; skills?: unknown; agent_profiles?: unknown } };
      const settings = parsed.settings ?? {};
      const allowedKeys = new Set(["profile.account", "ui.shortcuts", "ui.settings_tab", "ui.theme", "ui.locale", "ui.preferences", "agent.preferences", "model.preferences", "model.provider.preferences", "knowledge.preferences", "knowledge.postgres", "onboarding.completed", "onboarding.retrieval"]);
      const imported: Record<string, string | null> = {};
      let skipped = 0;
      let failed = 0;
      for (const [key, value] of Object.entries(settings)) {
        if (!allowedKeys.has(key) || typeof value !== "string") { skipped += 1; continue; }
        try {
          const parsedValue = JSON.parse(value);
          if (parsedValue === null || typeof parsedValue !== "object" || Array.isArray(parsedValue)) { skipped += 1; continue; }
          const normalized = key === "agent.preferences" ? JSON.stringify({ version: 3, ...normalizeAgentPreferences(value) }) : value;
          await persistSetting(key, normalized);
          imported[key] = normalized;
          if (key === "profile.account") {
            const account = parsedValue as Record<string, unknown>;
            if (typeof account.display_name === "string" && account.display_name.trim()) setAccountName(account.display_name.trim());
          }
          if (key === "ui.shortcuts") {
            const shortcuts = parsedValue as Record<string, unknown>;
            if (typeof shortcuts.send === "string" && shortcuts.send.trim()) setShortcutSend(shortcuts.send);
          }
        } catch {
          failed += 1;
        }
      }
      // Accept the small v1 export format while keeping a complete v2 profile intact.
      if (!Object.prototype.hasOwnProperty.call(imported, "profile.account") && typeof settings.account === "string") {
        const value = JSON.stringify({ version: 1, display_name: settings.account.trim() || "Suna", workspace_name: "本地工作区" });
        setAccountName(settings.account.trim() || "Suna");
        await persistSetting("profile.account", value);
        imported["profile.account"] = value;
      }
      if (!Object.prototype.hasOwnProperty.call(imported, "ui.shortcuts") && typeof settings.shortcutSend === "string") {
        const value = JSON.stringify({ version: 1, send: settings.shortcutSend });
        setShortcutSend(settings.shortcutSend);
        await persistSetting("ui.shortcuts", value);
        imported["ui.shortcuts"] = value;
      }
      let extensionImported = 0;
      let extensionSkipped = 0;
      if (isDesktopRuntime() && parsed.extensions) {
        const agents = Array.isArray(parsed.extensions.agent_profiles) ? parsed.extensions.agent_profiles : [];
        for (const entry of agents) {
          if (!entry || typeof entry !== "object" || Array.isArray(entry)) { extensionSkipped += 1; continue; }
          try { await desktop.saveAgentProfile(entry as AgentProfileSummary); extensionImported += 1; }
          catch { failed += 1; }
        }
        const skills = Array.isArray(parsed.extensions.skills) ? parsed.extensions.skills : [];
        for (const entry of skills) {
          if (!entry || typeof entry !== "object" || typeof (entry as { name?: unknown }).name !== "string" || typeof (entry as { enabled?: unknown }).enabled !== "boolean") { extensionSkipped += 1; continue; }
          try { await desktop.setSkillEnabled((entry as { name: string }).name, (entry as { enabled: boolean }).enabled); extensionImported += 1; } catch { extensionSkipped += 1; }
        }
        const servers = Array.isArray(parsed.extensions.mcp_servers) ? parsed.extensions.mcp_servers : [];
        for (const entry of servers) {
          if (!entry || typeof entry !== "object") { extensionSkipped += 1; continue; }
          const server = entry as Record<string, unknown>;
          if (typeof server.display_name !== "string" || typeof server.server_id !== "string" || (server.transport !== "stdio" && server.transport !== "streamable_http" && server.transport !== "sse")) { extensionSkipped += 1; continue; }
          try {
            await desktop.saveMcpServer({
              id: typeof server.id === "string" ? server.id : null,
              display_name: server.display_name,
              server_id: server.server_id,
              transport: server.transport,
              url: typeof server.url === "string" ? server.url : null,
              executable: typeof server.executable === "string" ? server.executable : null,
              args: Array.isArray(server.args) ? server.args.filter((value): value is string => typeof value === "string") : [],
              working_directory: typeof server.working_directory === "string" ? server.working_directory : null,
              inherited_env: Array.isArray(server.inherited_env) ? server.inherited_env.filter((value): value is string => typeof value === "string") : [],
              replace_inherited_env: true,
              env_values: {},
              timeout_ms: typeof server.timeout_ms === "number" ? server.timeout_ms : 30_000,
              enabled: server.enabled === true,
            });
            extensionImported += 1;
          } catch { extensionSkipped += 1; }
        }
      }
      setSettingsSnapshot((current) => ({ ...current, ...imported }));
      const importedCount = Object.keys(imported).length;
      setNotice(`已导入 ${importedCount + extensionImported} 项设置，跳过 ${skipped + extensionSkipped} 项，失败 ${failed} 项`);
    } catch (cause) { setError(settingsErrorMessage(cause, t("settingsImportError"))); }
    setFileInputKey((key) => key + 1);
  };
  useEffect(() => {
    void load();
  }, []);
  useEffect(() => {
    if (initialTab) setActiveTab(initialTab);
  }, [initialTab]);
  const persistRetrieval = async (nextPlan: RetrievalPlan, ids: RetrievalIds) => {
    await setSettingValue("onboarding.retrieval", JSON.stringify({
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
      setError(settingsErrorMessage(cause, t("settingsSaveError")));
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
      const providerMeta = parseObject(await getSettingValue("model.provider.preferences"));
      providerMeta[saved.id] = { temperature: editor.temperature, max_tokens: editor.maxTokens };
      await setSettingValue("model.provider.preferences", JSON.stringify({ version: 1, ...providerMeta }));
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
      setError(settingsErrorMessage(cause, t("settingsSaveError")));
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
      setError(settingsErrorMessage(cause, t("settingsTestFailed")));
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
      setError(settingsErrorMessage(cause, t("settingsDeleteError")));
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
      setError(settingsErrorMessage(cause, t("settingsSaveError")));
    } finally {
      setPermissionBusyId(null);
    }
  };
  const selectTab = (next: SettingsTab) => {
    setActiveTab(next);
    const value = JSON.stringify({ version: 1, tab: next });
    setSettingsSnapshot((current) => ({ ...current, "ui.settings_tab": value }));
    if (typeof window !== "undefined") {
      window.sessionStorage.setItem("suna.settings.tab", next);
      window.localStorage.setItem("suna.settings.tab", next);
    }
    // Keep the selected section in the authoritative desktop settings store
    // as well as the browser cache. Without this write, a desktop restart
    // loses the user's last settings section even though `load()` reads
    // `ui.settings_tab` from SQLite.
    void persistSetting("ui.settings_tab", value).catch((cause) => {
      setError(settingsErrorMessage(cause, t("settingsSaveError")));
    });
  };
  return (
    <section className="suna-settings-page suna-page-surface" aria-labelledby="settings-heading">
      <SettingsPageChrome t={t} loading={loading} error={error} notice={notice} query={query} setQuery={setQuery} load={() => void load()} exportSettings={exportSettings} importSettings={(file) => void importSettings(file)} fileInputKey={fileInputKey} resetPreferences={() => void resetPreferences()} onOpenDiagnostics={onOpenDiagnostics} showUtilities={activeTab !== "appearance"} />
      <div className={`suna-settings-layout ${activeTab === "appearance" ? "is-appearance" : ""}`}>
         <SettingsTabList tabs={visibleTabs} activeTab={activeTab} onSelect={selectTab} />
        <div role="tabpanel" id={`settings-panel-${activeTab}`} aria-labelledby={`settings-tab-${activeTab}`}>
          <SettingsPagePanel activeTab={activeTab} accountName={accountName} setAccountName={setAccountName} saveAccount={() => void saveAccount()} shortcutSend={shortcutSend} saveShortcut={(value) => void saveShortcut(value)} plan={plan} loading={loading} editors={editors} busySlot={busySlot} testingSlot={testingSlot} updateEditor={updateEditor} saveEditor={saveEditor} testEditor={(editor) => void testEditor(editor)} deleteEditor={(editor) => void deleteEditor(editor)} changePlan={(nextPlan) => void changePlan(nextPlan)} permissionRules={permissionRules} permissionBusyId={permissionBusyId} revokePermission={(rule) => void revokePermission(rule)} t={t} onOpenDiagnostics={onOpenDiagnostics} />
        </div>
      </div>
    </section>
  );
}
