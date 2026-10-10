import { useEffect, useRef, useState } from "react";
import { Check, CircleAlert, LoaderCircle, RefreshCw, RotateCcw, SlidersHorizontal } from "lucide-react";
import { desktop, isDesktopRuntime, type PostgresKnowledgeBase, type ProviderKind, type ProviderProfileResponse } from "../../bridge/desktop";
import { getSettingValue, parseObject, setSettingValue } from "./settingsModel";
import { settingsErrorMessage } from "./settingsError";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

type KnowledgePreferences = {
  defaultKnowledgeBase: string;
  embeddingProfileId: string | null;
  chunkSize: number;
  chunkOverlap: number;
  topK: number;
  similarityThreshold: number;
  rerankerEnabled: boolean;
  citationsEnabled: boolean;
  autoRetrieve: boolean;
  degradationPolicy: "fallback" | "strict";
};

const defaults: KnowledgePreferences = {
  defaultKnowledgeBase: "",
  embeddingProfileId: null,
  chunkSize: 512,
  chunkOverlap: 80,
  topK: 8,
  similarityThreshold: 0.7,
  rerankerEnabled: true,
  citationsEnabled: true,
  autoRetrieve: false,
  degradationPolicy: "fallback",
};

function normalize(raw: string | null, onboardingEmbeddingProfileId: string | null = null): KnowledgePreferences {
  const value = parseObject(raw);
  return {
    defaultKnowledgeBase: typeof value.default_knowledge_base === "string" ? value.default_knowledge_base : defaults.defaultKnowledgeBase,
    embeddingProfileId: typeof value.embedding_profile_id === "string" && value.embedding_profile_id.trim()
      ? value.embedding_profile_id
      : onboardingEmbeddingProfileId,
    chunkSize: typeof value.chunk_size === "number" ? Math.min(4096, Math.max(128, Math.round(value.chunk_size))) : defaults.chunkSize,
    chunkOverlap: typeof value.chunk_overlap === "number" ? Math.min(1024, Math.max(0, Math.round(value.chunk_overlap))) : defaults.chunkOverlap,
    topK: typeof value.top_k === "number" ? Math.min(50, Math.max(1, Math.round(value.top_k))) : defaults.topK,
    similarityThreshold: typeof value.similarity_threshold === "number" ? Math.min(1, Math.max(0, value.similarity_threshold)) : defaults.similarityThreshold,
    rerankerEnabled: typeof value.reranker_enabled === "boolean" ? value.reranker_enabled : defaults.rerankerEnabled,
    citationsEnabled: typeof value.citations_enabled === "boolean" ? value.citations_enabled : defaults.citationsEnabled,
    autoRetrieve: typeof value.auto_retrieve === "boolean" ? value.auto_retrieve : defaults.autoRetrieve,
    degradationPolicy: value.degradation_policy === "strict" ? "strict" : defaults.degradationPolicy,
  };
}

function serialize(value: KnowledgePreferences) {
  return JSON.stringify({
    version: 1,
    default_knowledge_base: value.defaultKnowledgeBase.trim(),
    embedding_profile_id: value.embeddingProfileId,
    chunk_size: value.chunkSize,
    chunk_overlap: value.chunkOverlap,
    top_k: value.topK,
    similarity_threshold: value.similarityThreshold,
    reranker_enabled: value.rerankerEnabled,
    citations_enabled: value.citationsEnabled,
    auto_retrieve: value.autoRetrieve,
    degradation_policy: value.degradationPolicy,
  });
}

function supportsEmbedding(kind: ProviderKind) {
  return kind === "open_ai_compatible" || kind === "qwen" || kind === "ollama" || kind === "siliconflow";
}

function configuredEmbeddingProfiles(profiles: ProviderProfileResponse[]) {
  return profiles.filter((profile) => (
    profile.enabled
    && (profile.secret_configured || profile.kind === "ollama")
    && supportsEmbedding(profile.kind)
    && Boolean(profile.model_id?.trim())
  ));
}

function Toggle({ checked, label, description, onChange }: { checked: boolean; label: string; description: string; onChange: (value: boolean) => void }) {
  return <label className="suna-settings-toggle-row"><span><strong>{label}</strong><small>{description}</small></span><Checkbox aria-label={label} checked={checked} onCheckedChange={(value) => onChange(value === true)} /></label>;
}

export default function KnowledgeRetrievalSettings() {
  const [value, setValue] = useState(defaults);
  const [knowledgeBases, setKnowledgeBases] = useState<PostgresKnowledgeBase[]>([]);
  const [providerProfiles, setProviderProfiles] = useState<ProviderProfileResponse[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [reloadToken, setReloadToken] = useState(0);
  const timer = useRef<number | null>(null);
  const pending = useRef(defaults);
  const retrievalSettings = useRef<Record<string, unknown>>({});

  useEffect(() => {
    let mounted = true;
    setLoaded(false);
    setLoadError(null);
    setSaveError(null);
    const listBases = isDesktopRuntime() && typeof desktop.listKnowledgeBases === "function"
      ? desktop.listKnowledgeBases()
      : Promise.resolve([] as PostgresKnowledgeBase[]);
    const listProfiles = isDesktopRuntime() && typeof desktop.listProviderProfiles === "function"
      ? desktop.listProviderProfiles()
      : Promise.resolve([] as ProviderProfileResponse[]);
    void Promise.all([
      getSettingValue("knowledge.preferences"),
      listBases,
      getSettingValue("onboarding.retrieval"),
      listProfiles,
    ]).then(([raw, bases, retrievalRaw, profiles]) => {
      if (!mounted) return;
      const retrieval = parseObject(retrievalRaw);
      retrievalSettings.current = retrieval;
      const onboardingEmbeddingProfileId = typeof retrieval.embedding_profile_id === "string" && retrieval.embedding_profile_id.trim()
        ? retrieval.embedding_profile_id
        : null;
      const next = normalize(raw, onboardingEmbeddingProfileId);
      pending.current = next;
      setValue(next);
      setKnowledgeBases(bases.filter((base) => base.id && base.name));
      setProviderProfiles(profiles);
    }).catch((cause) => {
      if (mounted) setLoadError(settingsErrorMessage(cause, "无法加载知识库检索设置"));
    }).finally(() => {
      if (mounted) setLoaded(true);
    });
    return () => {
      mounted = false;
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [reloadToken]);

  const persist = (next: KnowledgePreferences) => {
    setSaveError(null);
    setSaveState("saving");
    void setSettingValue("knowledge.preferences", serialize(next)).then(() => setSaveState("saved")).catch((cause) => {
      setSaveError(settingsErrorMessage(cause, "无法保存知识库检索设置"));
      setSaveState("error");
    });
  };

  const update = (partial: Partial<KnowledgePreferences>) => {
    const next = { ...pending.current, ...partial };
    next.chunkSize = Math.min(4096, Math.max(128, Math.round(next.chunkSize) || defaults.chunkSize));
    next.chunkOverlap = Math.min(next.chunkSize - 1, Math.max(0, Math.round(next.chunkOverlap) || 0));
    next.topK = Math.min(50, Math.max(1, Math.round(next.topK) || defaults.topK));
    next.similarityThreshold = Math.min(1, Math.max(0, Number.isFinite(next.similarityThreshold) ? next.similarityThreshold : defaults.similarityThreshold));
    pending.current = next;
    setValue(next);
    setSaveError(null);
    setSaveState("idle");
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      persist(next);
    }, 650);
  };

  const reset = () => {
    pending.current = defaults;
    setValue(defaults);
    setSaveState("idle");
    setSaveError(null);
    persist(defaults);
  };

  const selectEmbeddingProfile = (profileId: string) => {
    const nextProfileId = profileId.trim() || null;
    const previousProfileId = pending.current.embeddingProfileId;
    const next = { ...pending.current, embeddingProfileId: nextProfileId };
    pending.current = next;
    setValue(next);
    setSaveError(null);
    setSaveState("saving");
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }

    const persistSelection = async () => {
      if (isDesktopRuntime() && typeof desktop.setDefaultProvider === "function") {
        await desktop.setDefaultProvider("embedding", nextProfileId);
      }
      const nextRetrieval = { ...retrievalSettings.current, embedding_profile_id: nextProfileId };
      await setSettingValue("onboarding.retrieval", JSON.stringify(nextRetrieval));
      await setSettingValue("knowledge.preferences", serialize(next));
      retrievalSettings.current = nextRetrieval;
    };

    void persistSelection().then(() => setSaveState("saved")).catch(async (cause) => {
      pending.current = { ...pending.current, embeddingProfileId: previousProfileId };
      setValue(pending.current);
      setSaveError(settingsErrorMessage(cause, "无法保存 Embedding Provider"));
      setSaveState("error");
      if (isDesktopRuntime() && typeof desktop.setDefaultProvider === "function") {
        try {
          await desktop.setDefaultProvider("embedding", previousProfileId);
        } catch {
          // The original default is restored in the next successful settings load.
        }
      }
    });
  };

  const baseOptions = knowledgeBases.some((base) => base.id === value.defaultKnowledgeBase)
    ? knowledgeBases
    : value.defaultKnowledgeBase
      ? [{ id: value.defaultKnowledgeBase, name: `当前配置（${value.defaultKnowledgeBase}）` } as PostgresKnowledgeBase, ...knowledgeBases]
      : knowledgeBases;
  const embeddingProfiles = configuredEmbeddingProfiles(providerProfiles);
  const selectedEmbeddingProfile = embeddingProfiles.find((profile) => profile.id === value.embeddingProfileId);
  const staleEmbeddingProfile = value.embeddingProfileId && !selectedEmbeddingProfile;

  return <section className="suna-settings-form-panel suna-knowledge-retrieval-settings" aria-labelledby="knowledge-retrieval-heading" aria-busy={!loaded}>
    <header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">RETRIEVAL POLICY</span><h2 id="knowledge-retrieval-heading">知识库检索参数</h2><p>这些参数会作为新的知识检索和 Agent 引用请求的默认策略。</p></div><SlidersHorizontal size={22} aria-hidden="true" /></header>
    {!loaded || loadError ? <div className="suna-settings-state">{!loaded && <p className="suna-settings-loading" role="status" aria-live="polite"><LoaderCircle size={16} className="suna-spin" />正在加载检索设置...</p>}{loadError && <><p className="suna-settings-inline-error" role="alert"><CircleAlert size={14} />{loadError}</p><button type="button" className="suna-secondary-button" onClick={() => setReloadToken((value) => value + 1)}><RefreshCw size={14} />重新加载</button></>}</div> : <>
      <div className="suna-settings-form-grid">
        <label className="suna-settings-field"><span>默认知识库</span><Select value={value.defaultKnowledgeBase} onValueChange={(value) => update({ defaultKnowledgeBase: value })}>
  <SelectTrigger aria-label="默认知识库"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="">当前工作区（自动）</SelectItem>{baseOptions.map((base) => <SelectItem key={base.id} value={base.id}>{base.name}</SelectItem>)}
  </SelectContent>
</Select>{knowledgeBases.length === 0 && <small className="suna-settings-field-hint">暂无已连接的知识库，将使用当前工作区。</small>}</label>
        <div className="suna-settings-field"><span>Embedding Provider</span>{embeddingProfiles.length > 0 ? <Select value={value.embeddingProfileId ?? ""} onValueChange={(value) => selectEmbeddingProfile(value)}>
  <SelectTrigger aria-label="Embedding Provider"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="">未选择 Embedding Provider</SelectItem>{embeddingProfiles.map((profile) => <SelectItem key={profile.id} value={profile.id}>{profile.display_name} · {profile.model_id}</SelectItem>)}
  </SelectContent>
</Select> : <p className="suna-settings-empty-state" role="status">尚未配置可用的 Embedding Provider，请先在模型配置中添加已启用且已设置凭据的 Provider。</p>}{selectedEmbeddingProfile && <small className="suna-settings-field-hint">当前模型：{selectedEmbeddingProfile.model_id}</small>}{staleEmbeddingProfile && <small className="suna-settings-field-hint">当前选择的 Provider 已禁用、凭据缺失或不支持 Embedding，请重新选择。</small>}</div>
        <label className="suna-settings-field"><span>Chunk Size</span><Input type="number" min="128" max="4096" step="64" value={value.chunkSize} onChange={(event) => update({ chunkSize: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Chunk Overlap</span><Input type="number" min="0" max="1024" step="16" value={value.chunkOverlap} onChange={(event) => update({ chunkOverlap: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Top K</span><Input type="number" min="1" max="50" value={value.topK} onChange={(event) => update({ topK: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>Similarity Threshold</span><Input type="number" min="0" max="1" step="0.05" value={value.similarityThreshold} onChange={(event) => update({ similarityThreshold: Number(event.target.value) })} /></label>
        <label className="suna-settings-field"><span>检索失败策略</span><Select value={value.degradationPolicy} onValueChange={(value) => update({ degradationPolicy: value === "strict" ? "strict" : "fallback" })}>
  <SelectTrigger aria-label="检索失败策略"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="fallback">自动降级到全文检索</SelectItem><SelectItem value="strict">严格模式：直接返回错误</SelectItem>
  </SelectContent>
</Select></label>
      </div>
      <div className="suna-settings-subsection"><div className="suna-settings-subsection-heading"><SlidersHorizontal size={17} /><div><strong>检索增强</strong><small>控制自动检索、结果重排、Embedding 失败时如何处理，以及回答是否保留可验证来源。</small></div></div><div className="suna-settings-toggle-list"><Toggle checked={value.autoRetrieve} label="自动检索知识库" description="钢铁材料问题会在发送前自动检索所选知识库；问候和普通闲聊不会触发检索。" onChange={(autoRetrieve) => update({ autoRetrieve })} /><Toggle checked={value.rerankerEnabled} label="启用 Reranker" description="对混合检索候选进行二次排序。" onChange={(rerankerEnabled) => update({ rerankerEnabled })} /><Toggle checked={value.citationsEnabled} label="启用引用" description="在 Agent 回答中保留文档、页码和来源位置。" onChange={(citationsEnabled) => update({ citationsEnabled })} /></div></div>
      <footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={reset}><RotateCcw size={15} />恢复默认检索设置</button><div className={`suna-settings-retrieval-save-state is-${saveState}`} role="status">{saveState === "saving" && <LoaderCircle size={14} className="suna-spin" />}{saveState === "saved" && <Check size={14} />}{saveState === "error" ? (saveError ?? "保存失败，修改后将重试") : saveState === "saving" ? "正在保存..." : saveState === "saved" ? "已保存" : "自动保存"}</div></footer>
    </>}
  </section>;
}
