import { useEffect, useState, type FormEvent } from "react";
import {
  AlertCircle,
  Check,
  CircleHelp,
  KeyRound,
  LoaderCircle,
  Settings2,
} from "lucide-react";
import { desktop, type PermissionRuleRecord, type ProviderProfileInput } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import SettingsProviderCard from "./SettingsProviderCard";
import PermissionRulesPanel from "./PermissionRulesPanel";
import UpdatePanel from "./UpdatePanel";
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

export default function SettingsPage() {
  const { t } = useLocale();
  const [editors, setEditors] = useState<SettingsEditor[]>([]);
  const [plan, setPlan] = useState<RetrievalPlan>("free");
  const [retrievalIds, setRetrievalIds] = useState<RetrievalIds>(defaultRetrievalIds);
  const [permissionRules, setPermissionRules] = useState<PermissionRuleRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busySlot, setBusySlot] = useState<ProviderSlot | null>(null);
  const [testingSlot, setTestingSlot] = useState<ProviderSlot | null>(null);
  const [permissionBusyId, setPermissionBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

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
      setRetrievalIds(nextIds);
      setPermissionRules(permissions);
      setPlan(retrieval.plan === "pro" ? "pro" : "free");
      setEditors((Object.keys(defaults) as ProviderSlot[]).map((slot) =>
        editorFor(slot, profileForSlot(slot, profiles, completed, retrieval)),
      ));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsLoadError")));
    } finally {
      setLoading(false);
    }
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

  const changePlan = async (nextPlan: RetrievalPlan) => {
    setPlan(nextPlan);
    setError(null);
    try {
      await persistRetrieval(nextPlan, retrievalIds);
      setNotice(t("settingsSaved"));
    } catch (cause) {
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
      const nextIds = editor.slot === "chat"
        ? retrievalIds
        : { ...retrievalIds, [editor.slot]: saved.id } as RetrievalIds;
      setRetrievalIds(nextIds);
      await persistRetrieval(plan, nextIds);
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
      const result = await desktop.testProviderProfile(editor.id);
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
    <section className="bloomery-settings" aria-labelledby="settings-heading">
      <header className="bloomery-settings-header">
        <div>
          <p className="bloomery-eyebrow">LOCAL CONFIGURATION / PROVIDERS</p>
          <h1 id="settings-heading">{t("settingsTitle")}</h1>
          <p className="bloomery-lede">{t("settingsLede")}</p>
        </div>
        <button type="button" className="bloomery-icon-button" onClick={() => void load()} disabled={loading} aria-label={t("settingsRefresh")} title={t("settingsRefresh")}>
          <Settings2 size={18} aria-hidden="true" />
        </button>
      </header>

      {error && <div className="bloomery-settings-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-settings-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      <div className="bloomery-settings-safety">
        <KeyRound size={18} aria-hidden="true" />
        <div><strong>{t("settingsSecretTitle")}</strong><span>{t("settingsSecretCopy")}</span></div>
      </div>

      <UpdatePanel />

      <PermissionRulesPanel
        rules={permissionRules}
        busyId={permissionBusyId}
        onRevoke={(rule) => void revokePermission(rule)}
      />

      <section className="bloomery-settings-plan" aria-labelledby="settings-plan-heading">
        <div><p className="bloomery-eyebrow">RAG / SILICONFLOW</p><h2 id="settings-plan-heading">{t("settingsPlanTitle")}</h2><p>{t("settingsPlanCopy")}</p></div>
        <fieldset className="bloomery-settings-plan-options">
          <legend>{t("settingsPlanLabel")}</legend>
          <label><input type="radio" name="siliconflow-plan" checked={plan === "free"} onChange={() => void changePlan("free")} aria-label={t("settingsPlanFree")} />{t("freePlan")}</label>
          <label><input type="radio" name="siliconflow-plan" checked={plan === "pro"} onChange={() => void changePlan("pro")} aria-label={t("settingsPlanPro")} />{t("proPlan")}</label>
        </fieldset>
      </section>

      {loading ? <div className="bloomery-settings-loading"><LoaderCircle size={18} className="bloomery-spin" />{t("loading")}</div> : (
        <div className="bloomery-settings-grid">
          {editors.map((editor) => (
            <SettingsProviderCard
              key={editor.slot}
              editor={editor}
              busy={busySlot === editor.slot}
              testing={testingSlot === editor.slot}
              onChange={updateEditor}
              onSubmit={saveEditor}
              onTest={testEditor}
              onDelete={deleteEditor}
            />
          ))}
        </div>
      )}

      <aside className="bloomery-settings-note"><CircleHelp size={17} aria-hidden="true" /><span>{t("settingsProviderNote")}</span></aside>
    </section>
  );
}
