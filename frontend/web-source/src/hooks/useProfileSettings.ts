import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { AuthUserInfo } from "../LoginPage";
import type { LLMModelInfo } from "../types/llm";
import type {
  AuthSecurityConfigInfo,
  CaptchaAdminConfigInfo,
  CaptchaProviderValue,
  KnowledgeBaseSecurityConfigInfo,
  LLMConfigInfo,
  MinerUProcessingConfigInfo,
  MinerUUsageInfo,
  RetrievalModelsConfigInfo,
  TurnstileAdminConfigInfo,
  UserProfileInfo,
  UserProfileStatsInfo,
} from "../types/rag";
import {
  getAuthSecurityAdminConfig,
  getCaptchaAdminConfig,
  getKnowledgeBaseSecurityAdminConfig,
  getMinerUProcessingAdminConfig,
  getMinerUUsageAdminStatus,
  getRetrievalModelsAdminConfig,
  getTurnstileAdminConfig,
  getUserLlmConfig,
  getUserProfile,
  getUserProfileStats,
  postUserLlmModels,
  saveAuthSecurityAdminConfig as saveAuthSecurityAdminConfigRequest,
  saveCaptchaAdminConfig as saveCaptchaAdminConfigRequest,
  saveKnowledgeBaseSecurityAdminConfig as saveKnowledgeBaseSecurityAdminConfigRequest,
  saveMinerUProcessingAdminConfig as saveMinerUProcessingAdminConfigRequest,
  saveRetrievalModelsAdminConfig as saveRetrievalModelsAdminConfigRequest,
  saveTurnstileAdminConfig as saveTurnstileAdminConfigRequest,
  saveUserLlmConfig as saveUserLlmConfigRequest,
} from "../services/user";
import {
  DEFAULT_LLM_MODELS,
  LLM_MODEL_CACHE_TTL_MS,
  sortDoubaoModelsByDate,
} from "../services/llmModels";

type ProfileTab = "model" | "registration" | "knowledge" | "mineru" | "retrieval" | "captcha" | "asr";

interface UseProfileSettingsArgs {
  authChecked: boolean;
  isAuthenticated: boolean;
  authUser: AuthUserInfo | null;
  onBeforeOpenProfile?: () => void;
}

export interface UseProfileSettingsResult {
  showProfileCenter: boolean;
  setShowProfileCenter: Dispatch<SetStateAction<boolean>>;
  profileInfo: UserProfileInfo | null;
  setProfileInfo: Dispatch<SetStateAction<UserProfileInfo | null>>;
  profileSaving: boolean;
  securitySaving: boolean;
  profileError: string;
  setProfileError: Dispatch<SetStateAction<string>>;
  profileTab: ProfileTab;
  setProfileTab: Dispatch<SetStateAction<ProfileTab>>;
  llmProvider: string;
  setLlmProvider: Dispatch<SetStateAction<string>>;
  llmBaseUrl: string;
  setLlmBaseUrl: Dispatch<SetStateAction<string>>;
  llmModelName: string;
  setLlmModelName: Dispatch<SetStateAction<string>>;
  llmApiKey: string;
  setLlmApiKey: Dispatch<SetStateAction<string>>;
  llmDisplayName: string;
  setLlmDisplayName: Dispatch<SetStateAction<string>>;
  customProviderNames: Record<string, string>;
  activeLlmConfig: LLMConfigInfo | null;
  llmModels: LLMModelInfo[];
  setLlmModels: Dispatch<SetStateAction<LLMModelInfo[]>>;
  llmModelsLoading: boolean;
  llmModelsNote: string;
  turnstileConfig: TurnstileAdminConfigInfo | null;
  turnstileEnabled: boolean;
  setTurnstileEnabled: Dispatch<SetStateAction<boolean>>;
  turnstileSiteKey: string;
  setTurnstileSiteKey: Dispatch<SetStateAction<string>>;
  turnstileSecretKey: string;
  setTurnstileSecretKey: Dispatch<SetStateAction<string>>;
  turnstileSaving: boolean;
  captchaConfig: CaptchaAdminConfigInfo | null;
  captchaProvider: CaptchaProviderValue;
  setCaptchaProvider: Dispatch<SetStateAction<CaptchaProviderValue>>;
  geetestCaptchaId: string;
  setGeetestCaptchaId: Dispatch<SetStateAction<string>>;
  geetestPrivateKey: string;
  setGeetestPrivateKey: Dispatch<SetStateAction<string>>;
  saveCaptchaAdminConfig: (next?: { provider?: CaptchaProviderValue }) => Promise<void>;
  authSecurityConfig: AuthSecurityConfigInfo;
  setAuthSecurityConfig: Dispatch<SetStateAction<AuthSecurityConfigInfo>>;
  knowledgeBaseSecurityConfig: KnowledgeBaseSecurityConfigInfo;
  setKnowledgeBaseSecurityConfig: Dispatch<SetStateAction<KnowledgeBaseSecurityConfigInfo>>;
  mineruProcessingConfig: MinerUProcessingConfigInfo;
  setMineruProcessingConfig: Dispatch<SetStateAction<MinerUProcessingConfigInfo>>;
  mineruUsage: MinerUUsageInfo | null;
  mineruApiKey: string;
  setMineruApiKey: Dispatch<SetStateAction<string>>;
  retrievalModelsConfig: RetrievalModelsConfigInfo;
  setRetrievalModelsConfig: Dispatch<SetStateAction<RetrievalModelsConfigInfo>>;
  retrievalApiKey: string;
  setRetrievalApiKey: Dispatch<SetStateAction<string>>;
  profileUsername: string;
  profileInitial: string;
  isDslAdmin: boolean;
  llmDraftDirty: boolean;
  currentChatProvider: string;
  currentChatBaseUrl: string;
  currentChatModelName: string;
  activeModelDisplayName: string;
  availableChatModels: LLMModelInfo[];
  profileLlmModels: LLMModelInfo[];
  loadLlmModels: (provider: string, baseUrl: string, apiKey?: string, options?: { force?: boolean; quiet?: boolean }) => Promise<void>;
  loadLlmConfig: (provider: string) => Promise<LLMConfigInfo>;
  openProfileCenter: () => Promise<void>;
  saveProfileLlmConfig: () => Promise<void>;
  saveTurnstileAdminConfig: (next?: { enabled?: boolean; site_key?: string; secret_key?: string }) => Promise<void>;
  saveAuthSecurityConfig: (next?: AuthSecurityConfigInfo) => Promise<void>;
  saveKnowledgeBaseSecurityConfig: (next?: KnowledgeBaseSecurityConfigInfo) => Promise<void>;
  saveMineruProcessingConfig: (next?: MinerUProcessingConfigInfo, apiKey?: string) => Promise<void>;
  saveRetrievalModelsConfig: (next?: RetrievalModelsConfigInfo, apiKey?: string) => Promise<void>;
  switchAgentModel: (modelName: string) => Promise<void>;
}

export function useProfileSettings({
  authChecked,
  isAuthenticated,
  authUser,
  onBeforeOpenProfile,
}: UseProfileSettingsArgs): UseProfileSettingsResult {
  const [showProfileCenter, setShowProfileCenter] = useState(false);
  const [profileInfo, setProfileInfo] = useState<UserProfileInfo | null>(null);
  const [profileSaving, setProfileSaving] = useState(false);
  const [securitySaving, setSecuritySaving] = useState(false);
  const [profileError, setProfileError] = useState("");
  const [profileTab, setProfileTab] = useState<ProfileTab>("model");
  const [llmProvider, setLlmProvider] = useState("deepseek");
  const [llmBaseUrl, setLlmBaseUrl] = useState("");
  const [llmModelName, setLlmModelName] = useState("");
  const [llmApiKey, setLlmApiKey] = useState("");
  const [llmDisplayName, setLlmDisplayName] = useState("");
  const [customProviderNames, setCustomProviderNames] = useState<Record<string, string>>({});
  const [activeLlmConfig, setActiveLlmConfig] = useState<LLMConfigInfo | null>(null);
  const [llmModels, setLlmModels] = useState<LLMModelInfo[]>([]);
  const [llmModelsLoading, setLlmModelsLoading] = useState(false);
  const [llmModelsNote, setLlmModelsNote] = useState("");
  const llmModelsCacheRef = useRef<Record<string, { at: number; models: LLMModelInfo[]; source: string; error?: string }>>({});
  const agentModelSwitchSeqRef = useRef(0);
  const [turnstileConfig, setTurnstileConfig] = useState<TurnstileAdminConfigInfo | null>(null);
  const [turnstileEnabled, setTurnstileEnabled] = useState(false);
  const [turnstileSiteKey, setTurnstileSiteKey] = useState("");
  const [turnstileSecretKey, setTurnstileSecretKey] = useState("");
  const [turnstileSaving, setTurnstileSaving] = useState(false);
  const [captchaConfig, setCaptchaConfig] = useState<CaptchaAdminConfigInfo | null>(null);
  const [captchaProvider, setCaptchaProvider] = useState<CaptchaProviderValue>("none");
  const [geetestCaptchaId, setGeetestCaptchaId] = useState("");
  const [geetestPrivateKey, setGeetestPrivateKey] = useState("");
  const [authSecurityConfig, setAuthSecurityConfig] = useState<AuthSecurityConfigInfo>({
    registration_enabled: true,
    email_verify_enabled: true,
    password_reset_enabled: true,
    frontend_url: "",
  });
  const [knowledgeBaseSecurityConfig, setKnowledgeBaseSecurityConfig] = useState<KnowledgeBaseSecurityConfigInfo>({
    shared_enabled: false,
  });
  const [mineruProcessingConfig, setMineruProcessingConfig] = useState<MinerUProcessingConfigInfo>({
    provider_mode: "cloud_only",
    api_base: "https://mineru.net",
    model_version: "vlm",
    batch_size: 50,
    file_is_ocr: true,
    enable_formula: true,
    enable_table: true,
    api_key_configured: false,
    api_key_preview: "",
  });
  const [mineruUsage, setMineruUsage] = useState<MinerUUsageInfo | null>(null);
  const [mineruApiKey, setMineruApiKey] = useState("");
  const [retrievalModelsConfig, setRetrievalModelsConfig] = useState<RetrievalModelsConfigInfo>({
    provider_mode: "cloud_only",
    api_base: "https://api.siliconflow.cn",
    embedding_model: "BAAI/bge-m3",
    rerank_model: "BAAI/bge-reranker-v2-m3",
    api_key_configured: false,
    api_key_preview: "",
  });
  const [retrievalApiKey, setRetrievalApiKey] = useState("");

  const profileUsername = profileInfo?.username?.trim() || authUser?.username?.trim() || "用户";
  const profileInitial = profileUsername.slice(0, 1).toUpperCase() || "D";
  const isDslAdmin = profileUsername.toLowerCase() === "dsl" && String(profileInfo?.role || authUser?.role || "").toLowerCase() === "admin";
  const llmDraftDirty = Boolean(activeLlmConfig) && (
    llmProvider !== (activeLlmConfig?.provider || "") ||
    llmBaseUrl !== (activeLlmConfig?.base_url || "") ||
    llmModelName !== (activeLlmConfig?.model_name || "") ||
    llmDisplayName !== (activeLlmConfig?.display_name || "") ||
    Boolean(llmApiKey.trim())
  );

  const rememberCustomProviderName = useCallback((provider: string, name: string) => {
    if (!provider.startsWith("custom")) return;
    setCustomProviderNames(prev => (prev[provider] === name ? prev : { ...prev, [provider]: name }));
  }, []);

  const loadLlmModels = useCallback(async (provider: string, baseUrl: string, apiKey = "", options?: { force?: boolean; quiet?: boolean }) => {
    const providerKey = provider || "custom";
    const fallback = DEFAULT_LLM_MODELS[providerKey] ?? [];
    const cacheKey = `${providerKey}|${baseUrl || ""}|${apiKey.trim() ? "with-key" : "saved-key"}`;
    const cached = llmModelsCacheRef.current[cacheKey];
    if (!options?.force && cached && Date.now() - cached.at < LLM_MODEL_CACHE_TTL_MS) {
      setLlmModels(cached.models.length ? cached.models : fallback);
      setLlmModelsNote(cached.error && !options?.quiet ? cached.error : "");
      return;
    }
    if (!cached && !options?.quiet) setLlmModels(fallback);
    setLlmModelsLoading(!options?.quiet);
    if (!options?.quiet) setLlmModelsNote("");
    try {
      const resp = await postUserLlmModels({
        provider: providerKey,
        base_url: baseUrl || "",
        api_key: apiKey.trim() || null,
      });
      if (!resp.ok) throw new Error(await resp.text());
      const json: { models?: LLMModelInfo[]; source?: string; error?: string } = await resp.json();
      const models = Array.isArray(json.models) ? json.models : [];
      const nextModels = models.length ? models : fallback;
      llmModelsCacheRef.current[cacheKey] = { at: Date.now(), models: nextModels, source: json.source || "fallback", error: json.error || "" };
      setLlmModels(nextModels);
      setLlmModelsNote(json.error && !options?.quiet ? json.error : "");
    } catch (err: any) {
      llmModelsCacheRef.current[cacheKey] = { at: Date.now(), models: fallback, source: "fallback", error: err.message || String(err) };
      setLlmModels(fallback);
      setLlmModelsNote(options?.quiet ? "" : (err.message || String(err)));
    } finally {
      setLlmModelsLoading(false);
    }
  }, []);

  const loadLlmConfig = useCallback(async (provider: string) => {
    const params = new URLSearchParams({ provider });
    const resp = await getUserLlmConfig(params);
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as LLMConfigInfo;
    rememberCustomProviderName(cfg.provider || provider, cfg.display_name || "");
    return cfg;
  }, [rememberCustomProviderName]);

  const loadCustomProviderNames = useCallback(async () => {
    for (const key of ["custom", "custom2"]) {
      try {
        await loadLlmConfig(key);
      } catch { /* 名称加载失败不影响主流程 */ }
    }
  }, [loadLlmConfig]);

  useEffect(() => {
    if (!authChecked || !isAuthenticated) return;
    let cancelled = false;
    async function loadSavedLlmConfig() {
      try {
        const resp = await getUserLlmConfig();
        if (!resp.ok) throw new Error(await resp.text());
        const llm = await resp.json() as LLMConfigInfo;
        if (cancelled) return;
        setActiveLlmConfig(llm);
        setProfileInfo(prev => prev ? { ...prev, llm } : prev);
        setLlmProvider(llm.provider || "deepseek");
        setLlmBaseUrl(llm.base_url || "");
        setLlmModelName(llm.model_name || "");
        setLlmApiKey("");
        setLlmDisplayName(llm.display_name || "");
        rememberCustomProviderName(llm.provider || "", llm.display_name || "");
        void loadLlmModels(llm.provider || "deepseek", llm.base_url || "", "", { quiet: true });
      } catch (err) {
        console.warn("加载已保存模型配置失败", err);
      }
    }
    void loadSavedLlmConfig();
    return () => {
      cancelled = true;
    };
  }, [authChecked, isAuthenticated, loadLlmModels, rememberCustomProviderName]);

  const loadTurnstileAdminConfig = useCallback(async () => {
    const resp = await getTurnstileAdminConfig();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as TurnstileAdminConfigInfo;
    setTurnstileConfig(cfg);
    setTurnstileEnabled(Boolean(cfg.enabled));
    setTurnstileSiteKey(cfg.site_key || "");
    setTurnstileSecretKey("");
    return cfg;
  }, []);

  const loadCaptchaAdminConfig = useCallback(async () => {
    const resp = await getCaptchaAdminConfig();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as CaptchaAdminConfigInfo;
    setCaptchaConfig(cfg);
    setCaptchaProvider(cfg.provider || "none");
    setTurnstileEnabled(cfg.provider === "turnstile");
    setTurnstileSiteKey(cfg.turnstile_site_key || "");
    setTurnstileSecretKey("");
    setGeetestCaptchaId(cfg.geetest_captcha_id || "");
    setGeetestPrivateKey("");
    return cfg;
  }, []);

  const loadAuthSecurityConfig = useCallback(async () => {
    const resp = await getAuthSecurityAdminConfig();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as AuthSecurityConfigInfo;
    setAuthSecurityConfig({
      registration_enabled: cfg.registration_enabled !== false,
      email_verify_enabled: cfg.email_verify_enabled !== false,
      password_reset_enabled: cfg.password_reset_enabled !== false,
      frontend_url: cfg.frontend_url || "",
    });
    return cfg;
  }, []);

  const loadKnowledgeBaseSecurityConfig = useCallback(async () => {
    const resp = await getKnowledgeBaseSecurityAdminConfig();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as KnowledgeBaseSecurityConfigInfo;
    setKnowledgeBaseSecurityConfig({
      shared_enabled: cfg.shared_enabled === true,
    });
    return cfg;
  }, []);

  const loadMineruProcessingConfig = useCallback(async () => {
    const resp = await getMinerUProcessingAdminConfig();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as MinerUProcessingConfigInfo;
    setMineruProcessingConfig({
      provider_mode: "cloud_only",
      api_base: cfg.api_base || "https://mineru.net",
      model_version: cfg.model_version || "vlm",
      batch_size: cfg.batch_size || 50,
      file_is_ocr: cfg.file_is_ocr !== false,
      enable_formula: cfg.enable_formula !== false,
      enable_table: cfg.enable_table !== false,
      api_key_configured: cfg.api_key_configured === true,
      api_key_preview: cfg.api_key_preview || "",
    });
    setMineruApiKey("");
    return cfg;
  }, []);

  const loadMineruUsage = useCallback(async () => {
    const resp = await getMinerUUsageAdminStatus();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as MinerUUsageInfo;
    setMineruUsage(cfg);
    return cfg;
  }, []);

  const loadRetrievalModelsConfig = useCallback(async () => {
    const resp = await getRetrievalModelsAdminConfig();
    if (!resp.ok) throw new Error(await resp.text());
    const cfg = await resp.json() as RetrievalModelsConfigInfo;
    setRetrievalModelsConfig({
      provider_mode: "cloud_only",
      api_base: cfg.api_base || "https://api.siliconflow.cn",
      embedding_model: cfg.embedding_model || "BAAI/bge-m3",
      rerank_model: cfg.rerank_model || "BAAI/bge-reranker-v2-m3",
      api_key_configured: cfg.api_key_configured === true,
      api_key_preview: cfg.api_key_preview || "",
    });
    setRetrievalApiKey("");
    return cfg;
  }, []);

  const openProfileCenter = useCallback(async () => {
    onBeforeOpenProfile?.();
    setShowProfileCenter(true);
    setProfileError("");
    if (profileInfo) {
      setActiveLlmConfig(profileInfo.llm);
      setLlmProvider(profileInfo.llm.provider || "deepseek");
      setLlmBaseUrl(profileInfo.llm.base_url || "");
      setLlmModelName(profileInfo.llm.model_name || "");
      setLlmApiKey("");
      setLlmDisplayName(profileInfo.llm.display_name || "");
    }
    void loadCustomProviderNames();
    try {
      const profileResp = await getUserProfile();
      if (!profileResp.ok) throw new Error(await profileResp.text());
      const profileJson: UserProfileInfo = await profileResp.json();
      let mergedProfile = profileJson;
      try {
        const statsResp = await getUserProfileStats();
        if (statsResp.ok) {
          const statsJson = await statsResp.json() as UserProfileStatsInfo;
          mergedProfile = {
            ...profileJson,
            stats: statsJson.stats || profileJson.stats,
            traffic: statsJson.traffic || profileJson.traffic,
          };
        }
      } catch {
        // 统计刷新失败时保留 profile 接口的稳定数据，不打断配置使用。
      }
      setProfileInfo(mergedProfile);
      setActiveLlmConfig(mergedProfile.llm);
      setLlmProvider(mergedProfile.llm.provider || "deepseek");
      setLlmBaseUrl(mergedProfile.llm.base_url || "");
      setLlmModelName(mergedProfile.llm.model_name || "");
      setLlmApiKey("");
      setLlmDisplayName(mergedProfile.llm.display_name || "");
      if (String(mergedProfile.username || "").toLowerCase() === "dsl" && String(mergedProfile.role || "").toLowerCase() === "admin") {
        try {
          await Promise.all([loadTurnstileAdminConfig(), loadCaptchaAdminConfig(), loadAuthSecurityConfig(), loadKnowledgeBaseSecurityConfig(), loadMineruProcessingConfig(), loadMineruUsage(), loadRetrievalModelsConfig()]);
        } catch (err) {
          console.warn("加载安全配置失败", err);
        }
      } else {
        setTurnstileConfig(null);
        setTurnstileEnabled(false);
        setTurnstileSiteKey("");
        setTurnstileSecretKey("");
        setCaptchaConfig(null);
        setCaptchaProvider("none");
        setGeetestCaptchaId("");
        setGeetestPrivateKey("");
        setAuthSecurityConfig({
          registration_enabled: true,
          email_verify_enabled: true,
          password_reset_enabled: true,
          frontend_url: "",
        });
        setKnowledgeBaseSecurityConfig({ shared_enabled: false });
        setMineruProcessingConfig({
          provider_mode: "cloud_only",
          api_base: "https://mineru.net",
          model_version: "vlm",
          batch_size: 50,
          file_is_ocr: true,
          enable_formula: true,
          enable_table: true,
          api_key_configured: false,
          api_key_preview: "",
        });
        setMineruUsage(null);
        setMineruApiKey("");
        setRetrievalModelsConfig({
          provider_mode: "cloud_only",
          api_base: "https://api.siliconflow.cn",
          embedding_model: "BAAI/bge-m3",
          rerank_model: "BAAI/bge-reranker-v2-m3",
          api_key_configured: false,
          api_key_preview: "",
        });
        setRetrievalApiKey("");
      }
      void loadLlmModels(mergedProfile.llm.provider || "deepseek", mergedProfile.llm.base_url || "", "", { quiet: true });
    } catch (err: any) {
      setProfileError(err.message || String(err));
    }
  }, [loadAuthSecurityConfig, loadCustomProviderNames, loadKnowledgeBaseSecurityConfig, loadLlmModels, loadMineruProcessingConfig, loadMineruUsage, loadRetrievalModelsConfig, loadTurnstileAdminConfig, loadCaptchaAdminConfig, onBeforeOpenProfile, profileInfo]);

  const saveProfileLlmConfig = useCallback(async () => {
    setProfileSaving(true);
    setProfileError("");
    try {
      const resp = await saveUserLlmConfigRequest({
        provider: llmProvider,
        api_key: llmApiKey || null,
        base_url: llmBaseUrl,
        model_name: llmModelName,
        display_name: llmDisplayName.trim(),
      });
      if (!resp.ok) throw new Error(await resp.text());
      const llm = await resp.json() as LLMConfigInfo;
      setProfileInfo(prev => prev ? { ...prev, llm } : prev);
      setActiveLlmConfig(llm);
      setLlmApiKey("");
      setLlmDisplayName(llm.display_name || "");
      rememberCustomProviderName(llm.provider || llmProvider, llm.display_name || "");
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setProfileSaving(false);
    }
  }, [llmApiKey, llmBaseUrl, llmDisplayName, llmModelName, llmProvider, rememberCustomProviderName]);

  const saveTurnstileAdminConfig = useCallback(async (next?: { enabled?: boolean; site_key?: string; secret_key?: string }) => {
    const payload = {
      enabled: next?.enabled ?? turnstileEnabled,
      site_key: next?.site_key ?? turnstileSiteKey,
      secret_key: (next?.secret_key ?? turnstileSecretKey).trim() ? (next?.secret_key ?? turnstileSecretKey).trim() : null,
    };
    setTurnstileSaving(true);
    setProfileError("");
    try {
      const resp = await saveTurnstileAdminConfigRequest(payload);
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = await resp.json() as TurnstileAdminConfigInfo;
      setTurnstileConfig(cfg);
      setTurnstileEnabled(Boolean(cfg.enabled));
      setTurnstileSiteKey(cfg.site_key || "");
      setTurnstileSecretKey("");
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setTurnstileSaving(false);
    }
  }, [turnstileEnabled, turnstileSecretKey, turnstileSiteKey]);

  const saveCaptchaAdminConfig = useCallback(async (next?: { provider?: CaptchaProviderValue }) => {
    const provider = next?.provider ?? captchaProvider;
    const payload = {
      provider,
      turnstile_site_key: turnstileSiteKey,
      turnstile_secret_key: turnstileSecretKey.trim() ? turnstileSecretKey.trim() : null,
      geetest_captcha_id: geetestCaptchaId,
      geetest_private_key: geetestPrivateKey.trim() ? geetestPrivateKey.trim() : null,
    };
    setTurnstileSaving(true);
    setProfileError("");
    try {
      const resp = await saveCaptchaAdminConfigRequest(payload);
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = await resp.json() as CaptchaAdminConfigInfo;
      setCaptchaConfig(cfg);
      setCaptchaProvider(cfg.provider || "none");
      setTurnstileEnabled(cfg.provider === "turnstile");
      setTurnstileSiteKey(cfg.turnstile_site_key || "");
      setTurnstileSecretKey("");
      setGeetestCaptchaId(cfg.geetest_captcha_id || "");
      setGeetestPrivateKey("");
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setTurnstileSaving(false);
    }
  }, [captchaProvider, geetestCaptchaId, geetestPrivateKey, turnstileSecretKey, turnstileSiteKey]);

  const saveAuthSecurityConfig = useCallback(async (next?: AuthSecurityConfigInfo) => {
    const payload = next ?? authSecurityConfig;
    setSecuritySaving(true);
    setProfileError("");
    try {
      const resp = await saveAuthSecurityAdminConfigRequest(payload);
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = await resp.json() as AuthSecurityConfigInfo;
      setAuthSecurityConfig({
        registration_enabled: cfg.registration_enabled !== false,
        email_verify_enabled: cfg.email_verify_enabled !== false,
        password_reset_enabled: cfg.password_reset_enabled !== false,
        frontend_url: cfg.frontend_url || "",
      });
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setSecuritySaving(false);
    }
  }, [authSecurityConfig]);

  const saveKnowledgeBaseSecurityConfig = useCallback(async (next?: KnowledgeBaseSecurityConfigInfo) => {
    const payload = next ?? knowledgeBaseSecurityConfig;
    setSecuritySaving(true);
    setProfileError("");
    try {
      const resp = await saveKnowledgeBaseSecurityAdminConfigRequest(payload);
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = await resp.json() as KnowledgeBaseSecurityConfigInfo;
      setKnowledgeBaseSecurityConfig({ shared_enabled: cfg.shared_enabled === true });
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setSecuritySaving(false);
    }
  }, [knowledgeBaseSecurityConfig]);

  const saveMineruProcessingConfig = useCallback(async (next?: MinerUProcessingConfigInfo, apiKey?: string) => {
    const payload = next ?? mineruProcessingConfig;
    const secret = apiKey ?? mineruApiKey;
    setSecuritySaving(true);
    setProfileError("");
    try {
      const resp = await saveMinerUProcessingAdminConfigRequest({
        ...payload,
        api_key: secret.trim() ? secret.trim() : null,
      });
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = await resp.json() as MinerUProcessingConfigInfo;
      setMineruProcessingConfig({
        provider_mode: "cloud_only",
        api_base: cfg.api_base || "https://mineru.net",
        model_version: cfg.model_version || "vlm",
        batch_size: cfg.batch_size || 50,
        file_is_ocr: cfg.file_is_ocr !== false,
        enable_formula: cfg.enable_formula !== false,
        enable_table: cfg.enable_table !== false,
        api_key_configured: cfg.api_key_configured === true,
        api_key_preview: cfg.api_key_preview || "",
      });
      setMineruApiKey("");
      void loadMineruUsage().catch(() => undefined);
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setSecuritySaving(false);
    }
  }, [loadMineruUsage, mineruApiKey, mineruProcessingConfig]);

  const saveRetrievalModelsConfig = useCallback(async (next?: RetrievalModelsConfigInfo, apiKey?: string) => {
    const payload = next ?? retrievalModelsConfig;
    const secret = apiKey ?? retrievalApiKey;
    setSecuritySaving(true);
    setProfileError("");
    try {
      const resp = await saveRetrievalModelsAdminConfigRequest({
        ...payload,
        api_key: secret.trim() ? secret.trim() : null,
      });
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = await resp.json() as RetrievalModelsConfigInfo;
      setRetrievalModelsConfig({
        provider_mode: "cloud_only",
        api_base: cfg.api_base || "https://api.siliconflow.cn",
        embedding_model: cfg.embedding_model || "BAAI/bge-m3",
        rerank_model: cfg.rerank_model || "BAAI/bge-reranker-v2-m3",
        api_key_configured: cfg.api_key_configured === true,
        api_key_preview: cfg.api_key_preview || "",
      });
      setRetrievalApiKey("");
    } catch (err: any) {
      setProfileError(err.message || String(err));
    } finally {
      setSecuritySaving(false);
    }
  }, [retrievalApiKey, retrievalModelsConfig]);

  const savedLlmConfig = activeLlmConfig || profileInfo?.llm || null;
  const currentChatProvider = savedLlmConfig?.provider || llmProvider || "deepseek";
  const currentChatBaseUrl = savedLlmConfig?.base_url || llmBaseUrl || "";
  const currentChatModelName = savedLlmConfig?.model_name || llmModelName || "";

  const activeModelDisplayName = useMemo(() => currentChatModelName || "模型", [currentChatModelName]);

  const availableChatModels = useMemo(() => {
    const activeModel = currentChatModelName;
    const rawBase = llmModels.length ? llmModels : DEFAULT_LLM_MODELS[currentChatProvider] ?? [];
    const base = currentChatProvider === "doubao" ? sortDoubaoModelsByDate(rawBase) : rawBase;
    const merged = [...base];
    if (activeModel && !merged.some(model => model.id === activeModel)) {
      merged.unshift({ id: activeModel, name: activeModel, provider: currentChatProvider });
    }
    return merged;
  }, [currentChatModelName, currentChatProvider, llmModels]);

  const profileLlmModels = useMemo(
    () => llmProvider === "doubao" ? sortDoubaoModelsByDate(llmModels) : llmModels,
    [llmModels, llmProvider],
  );

  const switchAgentModel = useCallback(async (modelName: string) => {
    const nextModel = modelName.trim();
    if (!nextModel) return;
    const provider = currentChatProvider;
    const baseUrl = currentChatBaseUrl;
    const switchSeq = ++agentModelSwitchSeqRef.current;
    const previousModel = currentChatModelName;
    setProfileError("");
    setLlmProvider(provider);
    setLlmBaseUrl(baseUrl);
    setLlmModelName(nextModel);
    setActiveLlmConfig(prev => prev ? {
      ...prev,
      provider,
      base_url: baseUrl,
      model_name: nextModel,
    } : {
      provider,
      base_url: baseUrl,
      model_name: nextModel,
      api_key_configured: Boolean(profileInfo?.llm?.api_key_configured),
      api_key_preview: profileInfo?.llm?.api_key_preview || "",
    });
    setProfileInfo(prev => prev ? {
      ...prev,
      llm: {
        ...prev.llm,
        provider,
        base_url: baseUrl,
        model_name: nextModel,
      },
    } : prev);
    try {
      const resp = await saveUserLlmConfigRequest({
        provider,
        api_key: null,
        base_url: baseUrl,
        model_name: nextModel,
      });
      if (!resp.ok) throw new Error(await resp.text());
      const llm = await resp.json() as LLMConfigInfo;
      if (agentModelSwitchSeqRef.current === switchSeq) {
        setProfileInfo(prev => prev ? { ...prev, llm } : prev);
        setActiveLlmConfig(llm);
      }
    } catch (err: any) {
      if (agentModelSwitchSeqRef.current === switchSeq) {
        setLlmModelName(previousModel);
        setActiveLlmConfig(prev => prev ? { ...prev, model_name: previousModel } : prev);
        setProfileError(err.message || String(err));
      }
    }
  }, [currentChatBaseUrl, currentChatModelName, currentChatProvider, profileInfo?.llm?.api_key_configured, profileInfo?.llm?.api_key_preview]);

  return {
    showProfileCenter,
    setShowProfileCenter,
    profileInfo,
    setProfileInfo,
    profileSaving,
    securitySaving,
    profileError,
    setProfileError,
    profileTab,
    setProfileTab,
    llmProvider,
    setLlmProvider,
    llmBaseUrl,
    setLlmBaseUrl,
    llmModelName,
    setLlmModelName,
    llmApiKey,
    setLlmApiKey,
    llmDisplayName,
    setLlmDisplayName,
    customProviderNames,
    activeLlmConfig,
    llmModels,
    setLlmModels,
    llmModelsLoading,
    llmModelsNote,
    turnstileConfig,
    turnstileEnabled,
    setTurnstileEnabled,
    turnstileSiteKey,
    setTurnstileSiteKey,
    turnstileSecretKey,
    setTurnstileSecretKey,
    turnstileSaving,
    captchaConfig,
    captchaProvider,
    setCaptchaProvider,
    geetestCaptchaId,
    setGeetestCaptchaId,
    geetestPrivateKey,
    setGeetestPrivateKey,
    saveCaptchaAdminConfig,
    authSecurityConfig,
    setAuthSecurityConfig,
    knowledgeBaseSecurityConfig,
    setKnowledgeBaseSecurityConfig,
    mineruProcessingConfig,
    setMineruProcessingConfig,
    mineruUsage,
    mineruApiKey,
    setMineruApiKey,
    retrievalModelsConfig,
    setRetrievalModelsConfig,
    retrievalApiKey,
    setRetrievalApiKey,
    profileUsername,
    profileInitial,
    isDslAdmin,
    llmDraftDirty,
    currentChatProvider,
    currentChatBaseUrl,
    currentChatModelName,
    activeModelDisplayName,
    availableChatModels,
    profileLlmModels,
    loadLlmModels,
    loadLlmConfig,
    openProfileCenter,
    saveProfileLlmConfig,
    saveTurnstileAdminConfig,
    saveAuthSecurityConfig,
    saveKnowledgeBaseSecurityConfig,
    saveMineruProcessingConfig,
    saveRetrievalModelsConfig,
    switchAgentModel,
  };
}
