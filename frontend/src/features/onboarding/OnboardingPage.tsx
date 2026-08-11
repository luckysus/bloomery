import { useEffect, useState, type FormEvent } from "react";
import { desktop, type ProviderCapability, type ProviderProfileInput, type ProviderProfileResponse } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import OnboardingView, { type LlmForm, type RetrievalForm, type SetupStep } from "./OnboardingView";

interface OnboardingPageProps {
  onComplete: () => void;
}

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

const onboardingProgressKey = "onboarding.progress";

interface OnboardingProgress {
  version: 1;
  step: SetupStep | "done";
  llmProfileId: string | null;
  retrievalState: "configured" | "skipped";
  mineruConfigured: boolean;
}

type RestoredOnboardingProgress = Omit<OnboardingProgress, "step"> & { step: SetupStep };

function parseProgress(value: string | null): RestoredOnboardingProgress | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value) as Partial<OnboardingProgress>;
    if (parsed.version !== 1 || typeof parsed.step !== "string" || parsed.step === "done") return null;
    if (!["workspace", "llm", "retrieval", "finish"].includes(parsed.step)) return null;
    return {
      version: 1,
      step: parsed.step as SetupStep,
      llmProfileId: typeof parsed.llmProfileId === "string" ? parsed.llmProfileId : null,
      retrievalState: parsed.retrievalState === "configured" ? "configured" : "skipped",
      mineruConfigured: parsed.mineruConfigured === true,
    };
  } catch {
    return null;
  }
}

function providerError(errorCode: string | null | undefined, translate: (key: "credentialAuthentication" | "providerQuota" | "providerTimeout" | "providerNetwork" | "providerInvalidResponse") => string) {
  switch (errorCode) {
    case "authentication": return translate("credentialAuthentication");
    case "quota": return translate("providerQuota");
    case "timeout": return translate("providerTimeout");
    case "network": return translate("providerNetwork");
    default: return translate("providerInvalidResponse");
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
  const [llmProfileId, setLlmProfileId] = useState<string | null>(null);
  const [steelPackageState, setSteelPackageState] = useState<"pending" | "installing" | "installed" | "error">("pending");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const updateLlm = <K extends keyof LlmForm>(key: K, value: LlmForm[K]) => {
    setLlm((current) => ({ ...current, [key]: value }));
  };

  const updateRetrieval = <K extends keyof RetrievalForm>(key: K, value: RetrievalForm[K]) => {
    setRetrieval((current) => ({ ...current, [key]: value }));
  };

  const persistProgress = async (progress: Omit<OnboardingProgress, "version">) => {
    await desktop.setSetting(onboardingProgressKey, JSON.stringify({ version: 1, ...progress }));
  };

  const startSetup = () => {
    setError(null);
    setStep("llm");
    void persistProgress({ step: "llm", llmProfileId, retrievalState, mineruConfigured })
      .catch((cause) => setError(errorMessage(cause, t("setupError"))));
  };

  useEffect(() => {
    let mounted = true;
    const restore = async () => {
      try {
        const progress = parseProgress(await desktop.getSetting(onboardingProgressKey));
        if (!progress || progress.step === "workspace") return;
        let profile: ProviderProfileResponse | null = null;
        if (progress.llmProfileId) {
          const profiles = await desktop.listProviderProfiles();
          profile = profiles.find((item) => item.id === progress.llmProfileId) ?? null;
        }
        if (!mounted) return;
        setStep(progress.step);
        setLlmProfile(profile);
        setLlmProfileId(progress.llmProfileId);
        setRetrievalState(progress.retrievalState);
        setMineruConfigured(progress.mineruConfigured);
      } catch (cause) {
        if (mounted) setError(errorMessage(cause, t("setupError")));
      }
    };
    void restore();
    return () => {
      mounted = false;
    };
  }, [t]);

  const saveAndProbe = async (input: ProviderProfileInput, apiKey: string, capability: ProviderCapability) => {
    const profile = await desktop.saveProviderProfile(input);
    if (apiKey.trim()) await desktop.setProviderSecret(profile.id, "api_key", apiKey);
    const probe = await desktop.testProviderProfile(profile.id, capability);
    if (!probe.ok) throw new Error(providerError(probe.error_code, (key) => t(key)));
    await desktop.setDefaultProvider(capability, profile.id);
    return profile;
  };

  const handleLlmSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const profile = await saveAndProbe({ kind: llm.kind, display_name: llm.displayName, base_url: llm.baseUrl, model_id: llm.modelId, credential_name: llm.kind === "ollama" ? null : "api_key", enabled: true }, llm.apiKey, "chat");
      await persistProgress({ step: "retrieval", llmProfileId: profile.id, retrievalState, mineruConfigured });
      setLlmProfile(profile);
      setLlmProfileId(profile.id);
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
      const embedding = await saveAndProbe({ kind: "siliconflow", display_name: `SiliconFlow Embedding (${retrieval.plan})`, base_url: retrieval.baseUrl, model_id: retrieval.embeddingModel, credential_name: "api_key", enabled: true }, retrieval.siliconFlowKey, "embedding");
      const reranker = await saveAndProbe({ kind: "siliconflow", display_name: `SiliconFlow Reranker (${retrieval.plan})`, base_url: retrieval.baseUrl, model_id: retrieval.rerankerModel, credential_name: "api_key", enabled: true }, retrieval.siliconFlowKey, "rerank");
      let mineru: ProviderProfileResponse | null = null;
      if (retrieval.mineruKey.trim()) mineru = await saveAndProbe({ kind: "mineru", display_name: "MinerU", base_url: "https://mineru.net/api/v4", model_id: null, credential_name: "api_key", enabled: true }, retrieval.mineruKey, "document_parser");
      await desktop.setSetting("onboarding.retrieval", JSON.stringify({ state: "configured", plan: retrieval.plan, embedding_profile_id: embedding.id, reranker_profile_id: reranker.id, mineru_profile_id: mineru?.id ?? null }));
      await persistProgress({ step: "finish", llmProfileId, retrievalState: "configured", mineruConfigured: Boolean(mineru) });
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
      await persistProgress({ step: "finish", llmProfileId, retrievalState: "skipped", mineruConfigured: false });
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
    setSteelPackageState("installing");
    try {
      await desktop.installBundledSteelPackage();
      setSteelPackageState("installed");
      await desktop.setSetting("onboarding.completed", JSON.stringify({ version: 1, completed: true, llm_profile_id: llmProfileId ?? llmProfile?.id ?? null, retrieval_state: retrievalState }));
      await persistProgress({ step: "done", llmProfileId: llmProfileId ?? llmProfile?.id ?? null, retrievalState, mineruConfigured });
      onComplete();
    } catch (cause) {
      setSteelPackageState("error");
      setError(errorMessage(cause, t("setupError")));
    } finally {
      setBusy(false);
    }
  };

  return <OnboardingView
    step={step}
    llm={llm}
    retrieval={retrieval}
    retrievalState={retrievalState}
    mineruConfigured={mineruConfigured}
    steelPackageState={steelPackageState}
    busy={busy}
    error={error}
    onStartSetup={() => void startSetup()}
    onLlmChange={updateLlm}
    onRetrievalChange={updateRetrieval}
    onLlmSubmit={handleLlmSubmit}
    onRetrievalSubmit={handleRetrievalSubmit}
    onSkipRetrieval={() => void skipRetrieval()}
    onComplete={() => void complete()}
  />;
}
