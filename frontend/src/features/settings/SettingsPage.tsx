import { useEffect, useState, type FormEvent } from "react";
import {
  AlertCircle,
  Check,
  CircleHelp,
  KeyRound,
  LoaderCircle,
  PlugZap,
  Save,
  Settings2,
  Trash2,
} from "lucide-react";
import {
  desktop,
  type ProviderKind,
  type ProviderProfileInput,
  type ProviderProfileResponse,
} from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

type ProviderSlot = "chat" | "embedding" | "reranker" | "mineru";
type RetrievalPlan = "free" | "pro";

interface RetrievalIds {
  embedding: string | null;
  reranker: string | null;
  mineru: string | null;
}

interface SettingsEditor {
  slot: ProviderSlot;
  id: string | null;
  kind: ProviderKind;
  displayName: string;
  baseUrl: string;
  modelId: string;
  apiKey: string;
  enabled: boolean;
  secretConfigured: boolean;
}

const defaultRetrievalIds: RetrievalIds = {
  embedding: null,
  reranker: null,
  mineru: null,
};

const defaults: Record<ProviderSlot, Omit<SettingsEditor, "slot" | "id" | "apiKey" | "secretConfigured">> = {
  chat: {
    kind: "open_ai_compatible",
    displayName: "OpenAI Compatible",
    baseUrl: "https://api.openai.com/v1",
    modelId: "gpt-4o-mini",
    enabled: true,
  },
  embedding: {
    kind: "siliconflow",
    displayName: "SiliconFlow Embedding",
    baseUrl: "https://api.siliconflow.cn/v1",
    modelId: "BAAI/bge-m3",
    enabled: true,
  },
  reranker: {
    kind: "siliconflow",
    displayName: "SiliconFlow Reranker",
    baseUrl: "https://api.siliconflow.cn/v1",
    modelId: "BAAI/bge-reranker-v2-m3",
    enabled: true,
  },
  mineru: {
    kind: "mineru",
    displayName: "MinerU",
    baseUrl: "https://mineru.net/api/v4",
    modelId: "",
    enabled: true,
  },
};

const slotTitles: Record<ProviderSlot, "settingsChatProvider" | "settingsEmbeddingProvider" | "settingsRerankerProvider" | "settingsMineruProvider"> = {
  chat: "settingsChatProvider",
  embedding: "settingsEmbeddingProvider",
  reranker: "settingsRerankerProvider",
  mineru: "settingsMineruProvider",
};

const slotDescriptions: Record<ProviderSlot, "settingsChatDescription" | "settingsEmbeddingDescription" | "settingsRerankerDescription" | "settingsMineruDescription"> = {
  chat: "settingsChatDescription",
  embedding: "settingsEmbeddingDescription",
  reranker: "settingsRerankerDescription",
  mineru: "settingsMineruDescription",
};

function parseObject(value: string | null) {
  if (!value) return {} as Record<string, unknown>;
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" ? parsed as Record<string, unknown> : {};
  } catch {
    return {} as Record<string, unknown>;
  }
}

function parseId(value: unknown) {
  return typeof value === "string" && value.trim() ? value : null;
}

function profileForSlot(
  slot: ProviderSlot,
  profiles: ProviderProfileResponse[],
  completed: Record<string, unknown>,
  retrieval: Record<string, unknown>,
) {
  const configuredId = slot === "chat"
    ? parseId(completed.llm_profile_id)
    : parseId(retrieval[`${slot}_profile_id`]);
  const byId = configuredId ? profiles.find((profile) => profile.id === configuredId) : undefined;
  if (byId) return byId;

  return profiles.find((profile) => {
    if (slot === "chat") return profile.kind === "open_ai_compatible" || profile.kind === "ollama";
    if (slot === "mineru") return profile.kind === "mineru";
    if (profile.kind !== "siliconflow") return false;
    return slot === "embedding"
      ? profile.model_id?.toLowerCase().includes("bge-m3") && !profile.model_id.toLowerCase().includes("reranker")
      : profile.model_id?.toLowerCase().includes("rerank");
  });
}

function editorFor(
  slot: ProviderSlot,
  profile: ProviderProfileResponse | undefined,
): SettingsEditor {
  const fallback = defaults[slot];
  return {
    slot,
    id: profile?.id ?? null,
    kind: profile?.kind ?? fallback.kind,
    displayName: profile?.display_name ?? fallback.displayName,
    baseUrl: profile?.base_url ?? fallback.baseUrl,
    modelId: profile?.model_id ?? fallback.modelId,
    apiKey: "",
    enabled: profile?.enabled ?? fallback.enabled,
    secretConfigured: profile?.secret_configured ?? false,
  };
}

function providerErrorMessage(
  code: string | null | undefined,
  translate: (key: "credentialAuthentication" | "providerQuota" | "providerTimeout" | "providerNetwork" | "providerInvalidResponse") => string,
) {
  switch (code) {
    case "authentication":
      return translate("credentialAuthentication");
    case "quota":
      return translate("providerQuota");
    case "timeout":
      return translate("providerTimeout");
    case "network":
      return translate("providerNetwork");
    default:
      return translate("providerInvalidResponse");
  }
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

export default function SettingsPage() {
  const { t } = useLocale();
  const [editors, setEditors] = useState<SettingsEditor[]>([]);
  const [plan, setPlan] = useState<RetrievalPlan>("free");
  const [retrievalIds, setRetrievalIds] = useState<RetrievalIds>(defaultRetrievalIds);
  const [loading, setLoading] = useState(true);
  const [busySlot, setBusySlot] = useState<ProviderSlot | null>(null);
  const [testingSlot, setTestingSlot] = useState<ProviderSlot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const [profiles, completedValue, retrievalValue] = await Promise.all([
        desktop.listProviderProfiles(),
        desktop.getSetting("onboarding.completed"),
        desktop.getSetting("onboarding.retrieval"),
      ]);
      const completed = parseObject(completedValue);
      const retrieval = parseObject(retrievalValue);
      const nextIds: RetrievalIds = {
        embedding: parseId(retrieval.embedding_profile_id),
        reranker: parseId(retrieval.reranker_profile_id),
        mineru: parseId(retrieval.mineru_profile_id),
      };
      setRetrievalIds(nextIds);
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

  const updateEditor = <K extends keyof SettingsEditor>(slot: ProviderSlot, key: K, value: SettingsEditor[K]) => {
    setEditors((current) => current.map((editor) => editor.slot === slot ? { ...editor, [key]: value } : editor));
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
      setEditors((current) => current.map((item) => item.slot === editor.slot ? {
        ...item,
        id: saved.id,
        kind: saved.kind,
        displayName: saved.display_name,
        baseUrl: saved.base_url,
        modelId: saved.model_id ?? "",
        apiKey: "",
        enabled: saved.enabled,
        secretConfigured: saved.secret_configured || Boolean(editor.apiKey.trim()),
      } : item));
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
      setEditors((current) => current.map((item) => item.slot === editor.slot ? editorFor(editor.slot, undefined) : item));
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
            <form className="bloomery-settings-card" key={editor.slot} onSubmit={(event) => void saveEditor(event, editor)}>
              <div className="bloomery-settings-card-heading"><div><span className="bloomery-settings-card-icon"><PlugZap size={17} aria-hidden="true" /></span><div><p className="bloomery-eyebrow">{editor.kind.toUpperCase()}</p><h2>{t(slotTitles[editor.slot])}</h2></div></div><span className={`bloomery-settings-status ${editor.secretConfigured ? "is-configured" : "is-missing"}`}>{editor.secretConfigured ? t("settingsSecretConfigured") : t("settingsSecretMissing")}</span></div>
              <p className="bloomery-settings-description">{t(slotDescriptions[editor.slot])}</p>
              <div className="bloomery-settings-fields">
                <label htmlFor={`settings-${editor.slot}-name`}>{t("settingsDisplayName")}</label>
                <input id={`settings-${editor.slot}-name`} value={editor.displayName} onChange={(event) => updateEditor(editor.slot, "displayName", event.target.value)} required />
                <label htmlFor={`settings-${editor.slot}-url`}>{t("settingsBaseUrl")}</label>
                <input id={`settings-${editor.slot}-url`} value={editor.baseUrl} onChange={(event) => updateEditor(editor.slot, "baseUrl", event.target.value)} required />
                {editor.kind !== "mineru" && <><label htmlFor={`settings-${editor.slot}-model`}>{t("settingsModelId")}</label><input id={`settings-${editor.slot}-model`} value={editor.modelId} onChange={(event) => updateEditor(editor.slot, "modelId", event.target.value)} required /></>}
                <label htmlFor={`settings-${editor.slot}-key`}>{t("settingsApiKey")}</label>
                <input id={`settings-${editor.slot}-key`} aria-label={`provider.${editor.slot}.apiKey`} type="password" autoComplete="new-password" value={editor.apiKey} onChange={(event) => updateEditor(editor.slot, "apiKey", event.target.value)} placeholder={t("settingsApiKeyPlaceholder")} />
              </div>
              <label className="bloomery-settings-enabled"><input type="checkbox" checked={editor.enabled} onChange={(event) => updateEditor(editor.slot, "enabled", event.target.checked)} />{t("settingsEnabled")}</label>
              <div className="bloomery-settings-card-actions">
                <button type="submit" className="bloomery-action-primary" disabled={busySlot === editor.slot}><Save size={16} aria-hidden="true" />{busySlot === editor.slot ? t("saving") : t("settingsSave")}</button>
                <button type="button" className="bloomery-action-secondary" onClick={() => void testEditor(editor)} disabled={testingSlot === editor.slot || busySlot === editor.slot}><PlugZap size={16} aria-hidden="true" />{testingSlot === editor.slot ? t("testing") : t("settingsTest")}</button>
                {editor.id && <button type="button" className="bloomery-icon-button bloomery-settings-delete" onClick={() => void deleteEditor(editor)} disabled={busySlot === editor.slot} aria-label={`${t("settingsDelete")} ${editor.displayName}`} title={t("settingsDelete")}><Trash2 size={16} aria-hidden="true" /></button>}
              </div>
            </form>
          ))}
        </div>
      )}

      <aside className="bloomery-settings-note"><CircleHelp size={17} aria-hidden="true" /><span>{t("settingsProviderNote")}</span></aside>
    </section>
  );
}
