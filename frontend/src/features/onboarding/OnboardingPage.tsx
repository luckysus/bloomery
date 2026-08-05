import { useState, type FormEvent } from "react";
import { ArrowRight, Check, CircleAlert, Database, KeyRound, Server, ShieldCheck } from "lucide-react";
import { desktop, type ProviderKind, type ProviderProfileInput, type ProviderProfileResponse } from "../../bridge/desktop";
import LanguageSelect from "../../components/common/LanguageSelect";
import { useLocale } from "../../i18n/locale";

type SetupStep = "workspace" | "llm" | "retrieval" | "finish";

interface OnboardingPageProps {
  onComplete: () => void;
}

interface LlmForm {
  kind: Extract<ProviderKind, "open_ai_compatible" | "ollama">;
  displayName: string;
  baseUrl: string;
  modelId: string;
  apiKey: string;
}

interface RetrievalForm {
  enabled: boolean;
  plan: "free" | "pro";
  baseUrl: string;
  embeddingModel: string;
  rerankerModel: string;
  siliconFlowKey: string;
  mineruKey: string;
}

const steps: Array<{ id: SetupStep; labelKey: "stepWorkspace" | "stepLlm" | "stepRetrieval" | "stepFinish" }> = [
  { id: "workspace", labelKey: "stepWorkspace" },
  { id: "llm", labelKey: "stepLlm" },
  { id: "retrieval", labelKey: "stepRetrieval" },
  { id: "finish", labelKey: "stepFinish" },
];

const defaultLlm: LlmForm = {
  kind: "open_ai_compatible",
  displayName: "OpenAI Compatible",
  baseUrl: "https://api.openai.com/v1",
  modelId: "gpt-4o-mini",
  apiKey: "",
};

const defaultRetrieval: RetrievalForm = {
  enabled: true,
  plan: "free",
  baseUrl: "https://api.siliconflow.cn/v1",
  embeddingModel: "BAAI/bge-m3",
  rerankerModel: "BAAI/bge-reranker-v2-m3",
  siliconFlowKey: "",
  mineruKey: "",
};

function providerError(errorCode: string | null | undefined, translate: (key: "credentialAuthentication" | "providerQuota" | "providerTimeout" | "providerNetwork" | "providerInvalidResponse") => string) {
  switch (errorCode) {
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

export default function OnboardingPage({ onComplete }: OnboardingPageProps) {
  const { t } = useLocale();
  const [step, setStep] = useState<SetupStep>("workspace");
  const [llm, setLlm] = useState<LlmForm>(defaultLlm);
  const [retrieval, setRetrieval] = useState<RetrievalForm>(defaultRetrieval);
  const [llmProfile, setLlmProfile] = useState<ProviderProfileResponse | null>(null);
  const [retrievalState, setRetrievalState] = useState<"configured" | "skipped">("skipped");
  const [mineruConfigured, setMineruConfigured] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const updateLlm = <K extends keyof LlmForm>(key: K, value: LlmForm[K]) => {
    setLlm((current) => ({ ...current, [key]: value }));
  };

  const updateRetrieval = <K extends keyof RetrievalForm>(key: K, value: RetrievalForm[K]) => {
    setRetrieval((current) => ({ ...current, [key]: value }));
  };

  const saveAndProbe = async (input: ProviderProfileInput, apiKey: string) => {
    const profile = await desktop.saveProviderProfile(input);
    if (apiKey.trim()) {
      await desktop.setProviderSecret(profile.id, "api_key", apiKey);
    }
    const probe = await desktop.testProviderProfile(profile.id);
    if (!probe.ok) {
      throw new Error(providerError(probe.error_code, (key) => t(key)));
    }
    return profile;
  };

  const handleLlmSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const profile = await saveAndProbe(
        {
          kind: llm.kind,
          display_name: llm.displayName,
          base_url: llm.baseUrl,
          model_id: llm.modelId,
          credential_name: llm.kind === "ollama" ? null : "api_key",
          enabled: true,
        },
        llm.apiKey,
      );
      setLlmProfile(profile);
      setLlm((current) => ({ ...current, apiKey: "" }));
      setStep("retrieval");
    } catch (cause) {
      setError(errorMessage(cause, t("setupError")));
    } finally {
      setBusy(false);
    }
  };

  const handleRetrievalSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!retrieval.enabled) {
      setRetrievalState("skipped");
      setStep("finish");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const embedding = await saveAndProbe(
        {
          kind: "siliconflow",
          display_name: `SiliconFlow Embedding (${retrieval.plan})`,
          base_url: retrieval.baseUrl,
          model_id: retrieval.embeddingModel,
          credential_name: "api_key",
          enabled: true,
        },
        retrieval.siliconFlowKey,
      );
      const reranker = await saveAndProbe(
        {
          kind: "siliconflow",
          display_name: `SiliconFlow Reranker (${retrieval.plan})`,
          base_url: retrieval.baseUrl,
          model_id: retrieval.rerankerModel,
          credential_name: "api_key",
          enabled: true,
        },
        retrieval.siliconFlowKey,
      );
      let mineru: ProviderProfileResponse | null = null;
      if (retrieval.mineruKey.trim()) {
        mineru = await saveAndProbe(
          {
            kind: "mineru",
            display_name: "MinerU",
            base_url: "https://mineru.net/api/v4",
            model_id: null,
            credential_name: "api_key",
            enabled: true,
          },
          retrieval.mineruKey,
        );
      }
      await desktop.setSetting(
        "onboarding.retrieval",
        JSON.stringify({
          state: "configured",
          plan: retrieval.plan,
          embedding_profile_id: embedding.id,
          reranker_profile_id: reranker.id,
          mineru_profile_id: mineru?.id ?? null,
        }),
      );
      setRetrievalState("configured");
      setMineruConfigured(Boolean(mineru));
      setRetrieval((current) => ({ ...current, siliconFlowKey: "", mineruKey: "" }));
      setStep("finish");
    } catch (cause) {
      setError(errorMessage(cause, t("setupError")));
    } finally {
      setBusy(false);
    }
  };

  const skipRetrieval = async () => {
    setBusy(true);
    setError(null);
    try {
      await desktop.setSetting("onboarding.retrieval", JSON.stringify({ state: "skipped" }));
      setRetrievalState("skipped");
      setMineruConfigured(false);
      setStep("finish");
    } catch (cause) {
      setError(errorMessage(cause, t("setupError")));
    } finally {
      setBusy(false);
    }
  };

  const complete = async () => {
    setBusy(true);
    setError(null);
    try {
      await desktop.setSetting(
        "onboarding.completed",
        JSON.stringify({
          version: 1,
          completed: true,
          llm_profile_id: llmProfile?.id ?? null,
          retrieval_state: retrievalState,
        }),
      );
      onComplete();
    } catch (cause) {
      setError(errorMessage(cause, t("setupError")));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="bloomery-setup" aria-label={t("setupFirstRun")}>
      <div className="bloomery-setup-frame">
        <header className="bloomery-setup-header">
          <div className="bloomery-setup-brand">
            <span className="bloomery-setup-brand-mark" aria-hidden="true">
              <Server size={18} />
            </span>
            <div>
              <strong>BLOOMERY</strong>
              <span>LOCAL-FIRST STEEL AGENT</span>
            </div>
          </div>
          <span className="bloomery-setup-local-state">
            <span className="bloomery-state-dot" aria-hidden="true" />
            {t("dataOnlyLocal")}
          </span>
          <LanguageSelect />
        </header>

        <div className="bloomery-setup-layout">
          <aside className="bloomery-setup-progress" aria-label={t("setupProgress")}>
            <p className="bloomery-eyebrow">{t("setupFirstRunEyebrow")}</p>
            <h1>{t("setupTitle")}</h1>
            <p>{t("setupIntro")}</p>
            <ol>
              {steps.map((item, index) => {
                const active = item.id === step;
                const completeStep = steps.findIndex(({ id }) => id === step) > index;
                return (
                  <li key={item.id} className={active ? "is-active" : completeStep ? "is-complete" : ""}>
                    <span className="bloomery-setup-step-number" aria-hidden="true">
                      {completeStep ? <Check size={14} /> : index + 1}
                    </span>
                    <span>{t(item.labelKey)}</span>
                  </li>
                );
              })}
            </ol>
          </aside>

          <section className="bloomery-setup-panel">
            {error && (
              <div className="bloomery-setup-error" role="alert">
                <CircleAlert size={17} aria-hidden="true" />
                <span>{error}</span>
              </div>
            )}
            {step === "workspace" && (
              <section className="bloomery-setup-step" aria-labelledby="workspace-step-heading">
                <span className="bloomery-setup-icon" aria-hidden="true">
                  <Database size={21} />
                </span>
                <p className="bloomery-eyebrow">{t("storageStep")}</p>
                <h2 id="workspace-step-heading">{t("localWorkspaceTitle")}</h2>
                <p className="bloomery-setup-copy">{t("localWorkspaceCopy")}</p>
                <div className="bloomery-setup-fact">
                  <span>{t("dataDirectory")}</span>
                  <strong>{t("systemAppData")}</strong>
                </div>
                <button type="button" className="bloomery-setup-primary" onClick={() => setStep("llm")}>
                  {t("startSetup")}
                  <ArrowRight size={17} aria-hidden="true" />
                </button>
              </section>
            )}
            {step === "llm" && (
              <section className="bloomery-setup-step" aria-labelledby="llm-step-heading">
                <span className="bloomery-setup-icon" aria-hidden="true">
                  <KeyRound size={21} />
                </span>
                <p className="bloomery-eyebrow">{t("chatProviderStep")}</p>
                <h2 id="llm-step-heading">{t("connectLlm")}</h2>
                <p className="bloomery-setup-copy">{t("connectLlmCopy")}</p>
                <form className="bloomery-setup-form" onSubmit={handleLlmSubmit}>
                  <label>
                    {t("llmProvider")}
                    <select value={llm.kind} onChange={(event) => updateLlm("kind", event.target.value as LlmForm["kind"])}>
                      <option value="open_ai_compatible">OpenAI Compatible</option>
                      <option value="ollama">{t("ollamaLocal")}</option>
                    </select>
                  </label>
                  <label>
                    {t("displayName")}
                    <input value={llm.displayName} onChange={(event) => updateLlm("displayName", event.target.value)} required />
                  </label>
                  <label>
                    Base URL
                    <input value={llm.baseUrl} onChange={(event) => updateLlm("baseUrl", event.target.value)} required />
                  </label>
                  <label>
                    {t("modelId")}
                    <input value={llm.modelId} onChange={(event) => updateLlm("modelId", event.target.value)} required />
                  </label>
                  {llm.kind !== "ollama" && (
                    <label>
                      {t("apiKey")}
                      <input type="password" autoComplete="new-password" value={llm.apiKey} onChange={(event) => updateLlm("apiKey", event.target.value)} required />
                    </label>
                  )}
                  <button type="submit" className="bloomery-setup-primary" disabled={busy}>
                    {busy ? t("testing") : t("testLlmContinue")}
                    {!busy && <ArrowRight size={17} aria-hidden="true" />}
                  </button>
                </form>
              </section>
            )}
            {step === "retrieval" && (
              <section className="bloomery-setup-step" aria-labelledby="retrieval-step-heading">
                <span className="bloomery-setup-icon" aria-hidden="true">
                  <ShieldCheck size={21} />
                </span>
                <p className="bloomery-eyebrow">{t("retrievalStep")}</p>
                <h2 id="retrieval-step-heading">{t("retrievalTitle")}</h2>
                <p className="bloomery-setup-copy">{t("retrievalCopy")}</p>
                <form className="bloomery-setup-form" onSubmit={handleRetrievalSubmit}>
                  <label className="bloomery-setup-check-row">
                    <input type="checkbox" checked={retrieval.enabled} onChange={(event) => updateRetrieval("enabled", event.target.checked)} />
                    <span>{t("configureSiliconFlowNow")}</span>
                  </label>
                  {retrieval.enabled && (
                    <>
                      <label>
                        {t("siliconFlowPlan")}
                        <select value={retrieval.plan} onChange={(event) => updateRetrieval("plan", event.target.value as RetrievalForm["plan"])}>
                          <option value="free">{t("freePlan")}</option>
                          <option value="pro">{t("proPlan")}</option>
                        </select>
                      </label>
                      <label>
                        SiliconFlow Base URL
                        <input value={retrieval.baseUrl} onChange={(event) => updateRetrieval("baseUrl", event.target.value)} required />
                      </label>
                      <label>
                        {t("embeddingModel")}
                        <input value={retrieval.embeddingModel} onChange={(event) => updateRetrieval("embeddingModel", event.target.value)} required />
                      </label>
                      <label>
                        {t("rerankerModel")}
                        <input value={retrieval.rerankerModel} onChange={(event) => updateRetrieval("rerankerModel", event.target.value)} required />
                      </label>
                      <label>
                        {t("siliconFlowApiKey")}
                        <input type="password" autoComplete="new-password" value={retrieval.siliconFlowKey} onChange={(event) => updateRetrieval("siliconFlowKey", event.target.value)} required />
                      </label>
                    </>
                  )}
                  <label>
                    {t("mineruOptional")}
                    <input type="password" autoComplete="new-password" value={retrieval.mineruKey} onChange={(event) => updateRetrieval("mineruKey", event.target.value)} />
                  </label>
                  <div className="bloomery-setup-form-actions">
                    <button type="button" className="bloomery-setup-secondary" disabled={busy} onClick={skipRetrieval}>{t("skipForNow")}</button>
                    <button type="submit" className="bloomery-setup-primary" disabled={busy}>{busy ? t("testing") : t("saveContinue")}<ArrowRight size={17} aria-hidden="true" /></button>
                  </div>
                </form>
              </section>
            )}
            {step === "finish" && (
              <section className="bloomery-setup-step" aria-labelledby="finish-step-heading">
                <span className="bloomery-setup-icon is-success" aria-hidden="true">
                  <Check size={21} />
                </span>
                <p className="bloomery-eyebrow">{t("readyStep")}</p>
                <h2 id="finish-step-heading">{t("finishSetup")}</h2>
                <p className="bloomery-setup-copy">{t("finishCopy")}</p>
                <div className="bloomery-setup-summary">
                  <div><span>{t("llm")}</span><strong>{t("connected")}</strong></div>
                  <div><span>SiliconFlow</span><strong>{retrievalState === "configured" ? t("configured") : t("configureLater")}</strong></div>
                  <div><span>MinerU</span><strong>{mineruConfigured ? t("configured") : t("optional")}</strong></div>
                </div>
                <button type="button" className="bloomery-setup-primary" disabled={busy} onClick={complete}>
                  {busy ? t("saving") : t("enterWorkbench")}
                  {!busy && <ArrowRight size={17} aria-hidden="true" />}
                </button>
              </section>
            )}
          </section>
        </div>
      </div>
    </main>
  );
}
