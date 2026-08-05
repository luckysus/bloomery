import { useState, type FormEvent } from "react";
import { ArrowRight, Check, CircleAlert, Database, KeyRound, Server, ShieldCheck } from "lucide-react";
import { desktop, type ProviderKind, type ProviderProfileInput, type ProviderProfileResponse } from "../../bridge/desktop";

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

const steps: Array<{ id: SetupStep; label: string }> = [
  { id: "workspace", label: "本地工作区" },
  { id: "llm", label: "连接 LLM" },
  { id: "retrieval", label: "检索服务" },
  { id: "finish", label: "完成配置" },
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

function providerError(errorCode: string | null | undefined) {
  switch (errorCode) {
    case "authentication":
      return "凭据验证失败，请检查 API Key。";
    case "quota":
      return "服务商额度不足，请检查账户套餐或更换 Provider。";
    case "timeout":
      return "连接超时，请检查网络或服务地址。";
    case "network":
      return "无法连接服务商，请检查网络和 Base URL。";
    default:
      return "Provider 返回了无法使用的响应，请检查配置后重试。";
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "配置失败，请检查输入后重试。";
}

export default function OnboardingPage({ onComplete }: OnboardingPageProps) {
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
      throw new Error(providerError(probe.error_code));
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="bloomery-setup" aria-label="首次启动配置">
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
            数据仅存本机
          </span>
        </header>

        <div className="bloomery-setup-layout">
          <aside className="bloomery-setup-progress" aria-label="配置进度">
            <p className="bloomery-eyebrow">FIRST RUN</p>
            <h1>建立你的本地工作区</h1>
            <p>先连接模型，再决定是否启用检索服务。</p>
            <ol>
              {steps.map((item, index) => {
                const active = item.id === step;
                const completeStep = steps.findIndex(({ id }) => id === step) > index;
                return (
                  <li key={item.id} className={active ? "is-active" : completeStep ? "is-complete" : ""}>
                    <span className="bloomery-setup-step-number" aria-hidden="true">
                      {completeStep ? <Check size={14} /> : index + 1}
                    </span>
                    <span>{item.label}</span>
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
                <p className="bloomery-eyebrow">STEP 01 / STORAGE</p>
                <h2 id="workspace-step-heading">本地工作区</h2>
                <p className="bloomery-setup-copy">Bloomery 会使用 Windows 应用数据目录保存数据库、索引和任务状态。</p>
                <div className="bloomery-setup-fact">
                  <span>数据目录</span>
                  <strong>系统应用数据目录</strong>
                </div>
                <button type="button" className="bloomery-setup-primary" onClick={() => setStep("llm")}>
                  开始配置
                  <ArrowRight size={17} aria-hidden="true" />
                </button>
              </section>
            )}
            {step === "llm" && (
              <section className="bloomery-setup-step" aria-labelledby="llm-step-heading">
                <span className="bloomery-setup-icon" aria-hidden="true">
                  <KeyRound size={21} />
                </span>
                <p className="bloomery-eyebrow">STEP 02 / CHAT PROVIDER</p>
                <h2 id="llm-step-heading">连接 LLM</h2>
                <p className="bloomery-setup-copy">API Key 只会写入操作系统凭据库，不会进入 SQLite 或前端日志。</p>
                <form className="bloomery-setup-form" onSubmit={handleLlmSubmit}>
                  <label>
                    LLM 服务商
                    <select value={llm.kind} onChange={(event) => updateLlm("kind", event.target.value as LlmForm["kind"])}>
                      <option value="open_ai_compatible">OpenAI Compatible</option>
                      <option value="ollama">Ollama（本地）</option>
                    </select>
                  </label>
                  <label>
                    显示名称
                    <input value={llm.displayName} onChange={(event) => updateLlm("displayName", event.target.value)} required />
                  </label>
                  <label>
                    Base URL
                    <input value={llm.baseUrl} onChange={(event) => updateLlm("baseUrl", event.target.value)} required />
                  </label>
                  <label>
                    模型 ID
                    <input value={llm.modelId} onChange={(event) => updateLlm("modelId", event.target.value)} required />
                  </label>
                  {llm.kind !== "ollama" && (
                    <label>
                      API Key
                      <input type="password" autoComplete="new-password" value={llm.apiKey} onChange={(event) => updateLlm("apiKey", event.target.value)} required />
                    </label>
                  )}
                  <button type="submit" className="bloomery-setup-primary" disabled={busy}>
                    {busy ? "正在测试..." : "测试 LLM 并继续"}
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
                <p className="bloomery-eyebrow">STEP 03 / RETRIEVAL</p>
                <h2 id="retrieval-step-heading">检索服务</h2>
                <p className="bloomery-setup-copy">SiliconFlow 用于 BGE-M3 向量与 BGE Reranker 重排，MinerU 用于高质量 PDF 解析。两者都可以稍后配置。</p>
                <form className="bloomery-setup-form" onSubmit={handleRetrievalSubmit}>
                  <label className="bloomery-setup-check-row">
                    <input type="checkbox" checked={retrieval.enabled} onChange={(event) => updateRetrieval("enabled", event.target.checked)} />
                    <span>现在配置 SiliconFlow</span>
                  </label>
                  {retrieval.enabled && (
                    <>
                      <label>
                        SiliconFlow 套餐
                        <select value={retrieval.plan} onChange={(event) => updateRetrieval("plan", event.target.value as RetrievalForm["plan"])}>
                          <option value="free">免费版</option>
                          <option value="pro">Pro 版</option>
                        </select>
                      </label>
                      <label>
                        SiliconFlow Base URL
                        <input value={retrieval.baseUrl} onChange={(event) => updateRetrieval("baseUrl", event.target.value)} required />
                      </label>
                      <label>
                        Embedding 模型
                        <input value={retrieval.embeddingModel} onChange={(event) => updateRetrieval("embeddingModel", event.target.value)} required />
                      </label>
                      <label>
                        Reranker 模型
                        <input value={retrieval.rerankerModel} onChange={(event) => updateRetrieval("rerankerModel", event.target.value)} required />
                      </label>
                      <label>
                        SiliconFlow API Key
                        <input type="password" autoComplete="new-password" value={retrieval.siliconFlowKey} onChange={(event) => updateRetrieval("siliconFlowKey", event.target.value)} required />
                      </label>
                    </>
                  )}
                  <label>
                    MinerU API Key（可选）
                    <input type="password" autoComplete="new-password" value={retrieval.mineruKey} onChange={(event) => updateRetrieval("mineruKey", event.target.value)} />
                  </label>
                  <div className="bloomery-setup-form-actions">
                    <button type="button" className="bloomery-setup-secondary" disabled={busy} onClick={skipRetrieval}>暂时跳过</button>
                    <button type="submit" className="bloomery-setup-primary" disabled={busy}>{busy ? "正在测试..." : "保存并继续"}<ArrowRight size={17} aria-hidden="true" /></button>
                  </div>
                </form>
              </section>
            )}
            {step === "finish" && (
              <section className="bloomery-setup-step" aria-labelledby="finish-step-heading">
                <span className="bloomery-setup-icon is-success" aria-hidden="true">
                  <Check size={21} />
                </span>
                <p className="bloomery-eyebrow">STEP 04 / READY</p>
                <h2 id="finish-step-heading">完成配置</h2>
                <p className="bloomery-setup-copy">本地工作区已经可以使用。检索服务状态会在工作台和设置中持续显示。</p>
                <div className="bloomery-setup-summary">
                  <div><span>LLM</span><strong>已连接</strong></div>
                  <div><span>SiliconFlow</span><strong>{retrievalState === "configured" ? "已配置" : "稍后配置"}</strong></div>
                  <div><span>MinerU</span><strong>{mineruConfigured ? "已配置" : "可选"}</strong></div>
                </div>
                <button type="button" className="bloomery-setup-primary" disabled={busy} onClick={complete}>
                  {busy ? "正在保存..." : "进入工作台"}
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
