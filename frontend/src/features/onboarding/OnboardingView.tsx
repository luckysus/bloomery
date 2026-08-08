import type { FormEvent } from "react";
import { ArrowRight, Check, CircleAlert, Database, KeyRound, Server, ShieldCheck } from "lucide-react";
import { type ProviderKind } from "../../bridge/desktop";
import LanguageSelect from "../../components/common/LanguageSelect";
import { useLocale } from "../../i18n/locale";

export type SetupStep = "workspace" | "llm" | "retrieval" | "finish";

export interface LlmForm {
  kind: Extract<ProviderKind, "open_ai_compatible" | "ollama">;
  displayName: string;
  baseUrl: string;
  modelId: string;
  apiKey: string;
}

export interface RetrievalForm {
  enabled: boolean;
  plan: "free" | "pro";
  baseUrl: string;
  embeddingModel: string;
  rerankerModel: string;
  siliconFlowKey: string;
  mineruKey: string;
}

export interface OnboardingViewProps {
  step: SetupStep;
  llm: LlmForm;
  retrieval: RetrievalForm;
  retrievalState: "configured" | "skipped";
  mineruConfigured: boolean;
  busy: boolean;
  error: string | null;
  onStartSetup: () => void;
  onLlmChange: <K extends keyof LlmForm>(key: K, value: LlmForm[K]) => void;
  onRetrievalChange: <K extends keyof RetrievalForm>(key: K, value: RetrievalForm[K]) => void;
  onLlmSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onRetrievalSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onSkipRetrieval: () => void;
  onComplete: () => void;
}

const steps: Array<{ id: SetupStep; labelKey: "stepWorkspace" | "stepLlm" | "stepRetrieval" | "stepFinish" }> = [
  { id: "workspace", labelKey: "stepWorkspace" },
  { id: "llm", labelKey: "stepLlm" },
  { id: "retrieval", labelKey: "stepRetrieval" },
  { id: "finish", labelKey: "stepFinish" },
];

export default function OnboardingView({
  step,
  llm,
  retrieval,
  retrievalState,
  mineruConfigured,
  busy,
  error,
  onStartSetup,
  onLlmChange,
  onRetrievalChange,
  onLlmSubmit,
  onRetrievalSubmit,
  onSkipRetrieval,
  onComplete,
}: OnboardingViewProps) {
  const { t } = useLocale();

  return (
    <main className="bloomery-setup" aria-label={t("setupFirstRun")}>
      <div className="bloomery-setup-frame">
        <header className="bloomery-setup-header">
          <div className="bloomery-setup-brand"><span className="bloomery-setup-brand-mark" aria-hidden="true"><Server size={18} /></span><div><strong>BLOOMERY</strong><span>LOCAL-FIRST STEEL AGENT</span></div></div>
          <span className="bloomery-setup-local-state"><span className="bloomery-state-dot" aria-hidden="true" />{t("dataOnlyLocal")}</span>
          <LanguageSelect />
        </header>

        <div className="bloomery-setup-layout">
          <aside className="bloomery-setup-progress" aria-label={t("setupProgress")}>
            <p className="bloomery-eyebrow">{t("setupFirstRunEyebrow")}</p>
            <h1>{t("setupTitle")}</h1>
            <p>{t("setupIntro")}</p>
            <ol>{steps.map((item, index) => { const active = item.id === step; const complete = steps.findIndex(({ id }) => id === step) > index; return <li key={item.id} className={active ? "is-active" : complete ? "is-complete" : ""}><span className="bloomery-setup-step-number" aria-hidden="true">{complete ? <Check size={14} /> : index + 1}</span><span>{t(item.labelKey)}</span></li>; })}</ol>
          </aside>

          <section className="bloomery-setup-panel">
            {error && <div className="bloomery-setup-error" role="alert"><CircleAlert size={17} aria-hidden="true" /><span>{error}</span></div>}
            {step === "workspace" && <section className="bloomery-setup-step" aria-labelledby="workspace-step-heading"><span className="bloomery-setup-icon" aria-hidden="true"><Database size={21} /></span><p className="bloomery-eyebrow">{t("storageStep")}</p><h2 id="workspace-step-heading">{t("localWorkspaceTitle")}</h2><p className="bloomery-setup-copy">{t("localWorkspaceCopy")}</p><div className="bloomery-setup-fact"><span>{t("dataDirectory")}</span><strong>{t("systemAppData")}</strong></div><button type="button" className="bloomery-setup-primary" onClick={onStartSetup}>{t("startSetup")}<ArrowRight size={17} aria-hidden="true" /></button></section>}
            {step === "llm" && <section className="bloomery-setup-step" aria-labelledby="llm-step-heading"><span className="bloomery-setup-icon" aria-hidden="true"><KeyRound size={21} /></span><p className="bloomery-eyebrow">{t("chatProviderStep")}</p><h2 id="llm-step-heading">{t("connectLlm")}</h2><p className="bloomery-setup-copy">{t("connectLlmCopy")}</p><form className="bloomery-setup-form" onSubmit={onLlmSubmit}>
              <label>{t("llmProvider")}<select value={llm.kind} onChange={(event) => onLlmChange("kind", event.target.value as LlmForm["kind"])}><option value="open_ai_compatible">OpenAI Compatible</option><option value="ollama">{t("ollamaLocal")}</option></select></label>
              <label>{t("displayName")}<input value={llm.displayName} onChange={(event) => onLlmChange("displayName", event.target.value)} required /></label>
              <label>Base URL<input value={llm.baseUrl} onChange={(event) => onLlmChange("baseUrl", event.target.value)} required /></label>
              <label>{t("modelId")}<input value={llm.modelId} onChange={(event) => onLlmChange("modelId", event.target.value)} required /></label>
              {llm.kind !== "ollama" && <label>{t("apiKey")}<input type="password" autoComplete="new-password" value={llm.apiKey} onChange={(event) => onLlmChange("apiKey", event.target.value)} required /></label>}
              <button type="submit" className="bloomery-setup-primary" disabled={busy}>{busy ? t("testing") : t("testLlmContinue")}{!busy && <ArrowRight size={17} aria-hidden="true" />}</button>
            </form></section>}
            {step === "retrieval" && <section className="bloomery-setup-step" aria-labelledby="retrieval-step-heading"><span className="bloomery-setup-icon" aria-hidden="true"><ShieldCheck size={21} /></span><p className="bloomery-eyebrow">{t("retrievalStep")}</p><h2 id="retrieval-step-heading">{t("retrievalTitle")}</h2><p className="bloomery-setup-copy">{t("retrievalCopy")}</p><form className="bloomery-setup-form" onSubmit={onRetrievalSubmit}>
              <label className="bloomery-setup-check-row"><input type="checkbox" checked={retrieval.enabled} onChange={(event) => onRetrievalChange("enabled", event.target.checked)} /><span>{t("configureSiliconFlowNow")}</span></label>
              {retrieval.enabled && <><label>{t("siliconFlowPlan")}<select value={retrieval.plan} onChange={(event) => onRetrievalChange("plan", event.target.value as RetrievalForm["plan"])}><option value="free">{t("freePlan")}</option><option value="pro">{t("proPlan")}</option></select></label><label>{t("siliconFlowBaseUrl")}<input value={retrieval.baseUrl} onChange={(event) => onRetrievalChange("baseUrl", event.target.value)} required /></label><label>{t("embeddingModel")}<input value={retrieval.embeddingModel} onChange={(event) => onRetrievalChange("embeddingModel", event.target.value)} required /></label><label>{t("rerankerModel")}<input value={retrieval.rerankerModel} onChange={(event) => onRetrievalChange("rerankerModel", event.target.value)} required /></label><label>{t("siliconFlowApiKey")}<input type="password" autoComplete="new-password" value={retrieval.siliconFlowKey} onChange={(event) => onRetrievalChange("siliconFlowKey", event.target.value)} required /></label></>}
              <label>{t("mineruOptional")}<input type="password" autoComplete="new-password" value={retrieval.mineruKey} onChange={(event) => onRetrievalChange("mineruKey", event.target.value)} /></label>
              <div className="bloomery-setup-form-actions"><button type="button" className="bloomery-setup-secondary" disabled={busy} onClick={onSkipRetrieval}>{t("skipForNow")}</button><button type="submit" className="bloomery-setup-primary" disabled={busy}>{busy ? t("testing") : t("saveContinue")}<ArrowRight size={17} aria-hidden="true" /></button></div>
            </form></section>}
            {step === "finish" && <section className="bloomery-setup-step" aria-labelledby="finish-step-heading"><span className="bloomery-setup-icon is-success" aria-hidden="true"><Check size={21} /></span><p className="bloomery-eyebrow">{t("readyStep")}</p><h2 id="finish-step-heading">{t("finishSetup")}</h2><p className="bloomery-setup-copy">{t("finishCopy")}</p><div className="bloomery-setup-summary"><div><span>{t("llm")}</span><strong>{t("connected")}</strong></div><div><span>SiliconFlow</span><strong>{retrievalState === "configured" ? t("configured") : t("configureLater")}</strong></div><div><span>MinerU</span><strong>{mineruConfigured ? t("configured") : t("optional")}</strong></div></div><button type="button" className="bloomery-setup-primary" disabled={busy} onClick={onComplete}>{busy ? t("saving") : t("enterWorkbench")}{!busy && <ArrowRight size={17} aria-hidden="true" />}</button></section>}
          </section>
        </div>
      </div>
    </main>
  );
}
