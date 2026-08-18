import type { Dispatch, SetStateAction } from "react";
import {
  CircleUserRound,
  Gauge,
  Globe,
  KeyRound,
  Loader2,
  Mail,
  Network,
  RefreshCw,
  Save,
  ShieldCheck,
  UserPlus,
} from "lucide-react";
import SettingsSwitch from "../common/SettingsSwitch";
import type { LabServiceStatusInfo } from "../../services/labService";
import type { LLMModelInfo } from "../../types/llm";
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
} from "../../types/rag";
import { ProfileHeader } from "./ProfileHeader";
import { ProfileTabs, type ProfileTab } from "./ProfileTabs";
import AsrConfigManager from "./AsrConfigManager";

type LabCardTone = "slate" | "emerald" | "amber" | "red";

interface ProfileCenterProps {
  showProfileCenter: boolean;
  setShowProfileCenter: Dispatch<SetStateAction<boolean>>;
  profileInitial: string;
  profileUsername: string;
  profileTab: ProfileTab;
  setProfileTab: Dispatch<SetStateAction<ProfileTab>>;
  profileError: string;
  setProfileError: Dispatch<SetStateAction<string>>;
  llmDraftDirty: boolean;
  llmProvider: string;
  setLlmProvider: Dispatch<SetStateAction<string>>;
  llmApiKey: string;
  setLlmApiKey: Dispatch<SetStateAction<string>>;
  llmDisplayName: string;
  setLlmDisplayName: Dispatch<SetStateAction<string>>;
  customProviderNames: Record<string, string>;
  llmBaseUrl: string;
  setLlmBaseUrl: Dispatch<SetStateAction<string>>;
  llmModelName: string;
  setLlmModelName: Dispatch<SetStateAction<string>>;
  llmModelsLoading: boolean;
  profileLlmModels: LLMModelInfo[];
  llmModelsNote: string;
  setLlmModels: Dispatch<SetStateAction<LLMModelInfo[]>>;
  loadLlmConfig: (provider: string) => Promise<LLMConfigInfo>;
  loadLlmModels: (provider: string, baseUrl: string, apiKey?: string, options?: { force?: boolean; quiet?: boolean }) => Promise<void>;
  profileInfo: UserProfileInfo | null;
  saveProfileLlmConfig: () => void | Promise<void>;
  profileSaving: boolean;
  labCardTone: LabCardTone;
  labRetrievalAvailable: boolean;
  labOptimizationAvailable: boolean;
  labServiceStatus: LabServiceStatusInfo | null;
  handleLabServiceAction: () => void | Promise<void>;
  labServiceLoading: boolean;
  isDslAdmin: boolean;
  saveAuthSecurityConfig: (next?: AuthSecurityConfigInfo) => void | Promise<void>;
  securitySaving: boolean;
  authSecurityConfig: AuthSecurityConfigInfo;
  setAuthSecurityConfig: Dispatch<SetStateAction<AuthSecurityConfigInfo>>;
  knowledgeBaseSecurityConfig: KnowledgeBaseSecurityConfigInfo;
  setKnowledgeBaseSecurityConfig: Dispatch<SetStateAction<KnowledgeBaseSecurityConfigInfo>>;
  saveKnowledgeBaseSecurityConfig: (next?: KnowledgeBaseSecurityConfigInfo) => void | Promise<void>;
  mineruProcessingConfig: MinerUProcessingConfigInfo;
  mineruUsage: MinerUUsageInfo | null;
  setMineruProcessingConfig: Dispatch<SetStateAction<MinerUProcessingConfigInfo>>;
  mineruApiKey: string;
  setMineruApiKey: Dispatch<SetStateAction<string>>;
  saveMineruProcessingConfig: (next?: MinerUProcessingConfigInfo, apiKey?: string) => void | Promise<void>;
  retrievalModelsConfig: RetrievalModelsConfigInfo;
  setRetrievalModelsConfig: Dispatch<SetStateAction<RetrievalModelsConfigInfo>>;
  retrievalApiKey: string;
  setRetrievalApiKey: Dispatch<SetStateAction<string>>;
  saveRetrievalModelsConfig: (next?: RetrievalModelsConfigInfo, apiKey?: string) => void | Promise<void>;
  turnstileEnabled: boolean;
  setTurnstileEnabled: Dispatch<SetStateAction<boolean>>;
  turnstileSiteKey: string;
  setTurnstileSiteKey: Dispatch<SetStateAction<string>>;
  turnstileSecretKey: string;
  setTurnstileSecretKey: Dispatch<SetStateAction<string>>;
  turnstileConfig: TurnstileAdminConfigInfo | null;
  saveTurnstileAdminConfig: (next?: { enabled?: boolean; site_key?: string; secret_key?: string }) => void | Promise<void>;
  turnstileSaving: boolean;
  captchaConfig: CaptchaAdminConfigInfo | null;
  captchaProvider: CaptchaProviderValue;
  setCaptchaProvider: Dispatch<SetStateAction<CaptchaProviderValue>>;
  geetestCaptchaId: string;
  setGeetestCaptchaId: Dispatch<SetStateAction<string>>;
  geetestPrivateKey: string;
  setGeetestPrivateKey: Dispatch<SetStateAction<string>>;
  saveCaptchaAdminConfig: (next?: { provider?: CaptchaProviderValue }) => void | Promise<void>;
}

export default function ProfileCenter({
  showProfileCenter,
  setShowProfileCenter,
  profileUsername,
  profileTab,
  setProfileTab,
  profileError,
  setProfileError,
  llmDraftDirty,
  llmProvider,
  setLlmProvider,
  llmApiKey,
  setLlmApiKey,
  llmDisplayName,
  setLlmDisplayName,
  customProviderNames,
  llmBaseUrl,
  setLlmBaseUrl,
  llmModelName,
  setLlmModelName,
  llmModelsLoading,
  profileLlmModels,
  llmModelsNote,
  setLlmModels,
  loadLlmConfig,
  loadLlmModels,
  profileInfo,
  saveProfileLlmConfig,
  profileSaving,
  labCardTone,
  labRetrievalAvailable,
  labOptimizationAvailable,
  labServiceStatus,
  handleLabServiceAction,
  labServiceLoading,
  isDslAdmin,
  saveAuthSecurityConfig,
  securitySaving,
  authSecurityConfig,
  setAuthSecurityConfig,
  knowledgeBaseSecurityConfig,
  setKnowledgeBaseSecurityConfig,
  saveKnowledgeBaseSecurityConfig,
  mineruProcessingConfig,
  mineruUsage,
  setMineruProcessingConfig,
  mineruApiKey,
  setMineruApiKey,
  saveMineruProcessingConfig,
  retrievalModelsConfig,
  setRetrievalModelsConfig,
  retrievalApiKey,
  setRetrievalApiKey,
  saveRetrievalModelsConfig,
  turnstileEnabled,
  setTurnstileEnabled,
  turnstileSiteKey,
  setTurnstileSiteKey,
  turnstileSecretKey,
  setTurnstileSecretKey,
  turnstileConfig,
  saveTurnstileAdminConfig,
  turnstileSaving,
  captchaConfig,
  captchaProvider,
  setCaptchaProvider,
  geetestCaptchaId,
  setGeetestCaptchaId,
  geetestPrivateKey,
  setGeetestPrivateKey,
  saveCaptchaAdminConfig,
}: ProfileCenterProps) {
  const effectiveProfileTab = isDslAdmin ? profileTab : "model";

  return (
    <>
      {showProfileCenter && (
        <div className="fixed inset-0 z-[70] flex items-center justify-center bg-slate-950/25 px-4 max-md:px-0" onPointerDown={() => setShowProfileCenter(false)}>
          <div className="flex h-[min(760px,86vh)] w-full max-w-4xl flex-col overflow-hidden rounded-2xl bg-white shadow-2xl shadow-slate-950/20 max-md:h-[100dvh] max-md:max-w-full max-md:rounded-none" onPointerDown={(event) => event.stopPropagation()}>
            <ProfileHeader onClose={() => setShowProfileCenter(false)} />

            <div className="flex min-h-0 flex-1 overflow-hidden max-md:flex-col">
              <ProfileTabs profileTab={effectiveProfileTab} setProfileTab={setProfileTab} isDslAdmin={isDslAdmin} />
              <div className="min-h-0 flex-1 overflow-y-auto p-6 max-md:p-3">
                {profileError && (
                  <div className="mb-4 break-words rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-sm leading-relaxed text-red-600">
                    {profileError}
                  </div>
                )}
                <div className={`${effectiveProfileTab === "model" ? "grid" : "hidden"} gap-4 lg:grid-cols-[minmax(0,1.35fr)_minmax(320px,0.65fr)]`}>
                  <section className="min-w-0 rounded-xl border border-slate-200 bg-white p-4 max-md:p-3">
                    <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
                      <div className="flex items-center gap-2 text-base font-bold text-slate-900">
                        <KeyRound size={20} className="text-indigo-600" />
                        模型配置
                      </div>
                      {llmDraftDirty && (
                        <span className="rounded-full border border-amber-200 bg-amber-50 px-2.5 py-1 text-xs font-semibold text-amber-700">
                          未保存，当前对话仍使用已保存模型
                        </span>
                      )}
                    </div>
                    <div className="grid gap-3">
                      <label className="space-y-1.5">
                        <span className="text-sm font-semibold text-slate-600">供应商</span>
                        <select
                          value={llmProvider}
                          onChange={async (event) => {
                            const provider = event.target.value;
                            setLlmProvider(provider);
                            setLlmApiKey("");
                            setProfileError("");
                            try {
                              const cfg = await loadLlmConfig(provider);
                              setLlmDisplayName(cfg.display_name || "");
                              let nextBaseUrl = cfg.base_url || "";
                              let nextModelName = cfg.model_name || "";
                              if (provider === "deepseek") {
                                nextBaseUrl = nextBaseUrl || "https://api.deepseek.com";
                                nextModelName = nextModelName && !["deepseek-chat", "deepseek-reasoner"].includes(nextModelName)
                                  ? nextModelName
                                  : "deepseek-v4-flash";
                              } else if (provider === "doubao") {
                                nextBaseUrl = nextBaseUrl || "https://ark.cn-beijing.volces.com/api/v3";
                                nextModelName = nextModelName || "doubao-seed-2-0-pro";
                              } else if (provider === "opencode") {
                                nextBaseUrl = nextBaseUrl || "https://opencode.ai/zen/go/v1";
                                nextModelName = nextModelName || "deepseek-v4-pro";
                              }
                              setLlmBaseUrl(nextBaseUrl);
                              setLlmModelName(nextModelName);
                              loadLlmModels(provider, nextBaseUrl, "", { quiet: true });
                            } catch (err: any) {
                              setProfileError(err.message || String(err));
                            }
                          }}
                          className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-base text-slate-900 outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                        >
                          <option value="deepseek">DeepSeek</option>
                          <option value="doubao">豆包 / 火山方舟</option>
                          <option value="opencode">OpenCode Go</option>
                          <option value="custom">{customProviderNames["custom"]?.trim() || "自定义 1"}</option>
                          <option value="custom2">{customProviderNames["custom2"]?.trim() || "自定义 2"}</option>
                        </select>
                      </label>
                      {llmProvider === "opencode" && (
                        <label className="space-y-1.5">
                          <span className="text-sm font-semibold text-slate-600">显示名称</span>
                          <input
                            value={llmDisplayName}
                            onChange={(event) => setLlmDisplayName(event.target.value)}
                            maxLength={64}
                            placeholder="OpenCode Go"
                            className="h-10 w-full rounded-lg border border-slate-200 px-3 text-base outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                          />
                          <p className="text-xs leading-relaxed text-slate-400">这个名称会显示在模型下拉框中。</p>
                        </label>
                      )}
                      {llmProvider.startsWith("custom") && (
                        <label className="space-y-1.5">
                          <span className="text-sm font-semibold text-slate-600">显示名称</span>
                          <input
                            value={llmDisplayName}
                            onChange={(event) => setLlmDisplayName(event.target.value)}
                            maxLength={64}
                            placeholder={llmProvider === "custom2" ? "自定义 2" : "自定义 1"}
                            className="h-10 w-full rounded-lg border border-slate-200 px-3 text-base outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                          />
                          <p className="text-xs leading-relaxed text-slate-400">保存后供应商下拉会显示这个名字，方便区分两个自定义配置。</p>
                        </label>
                      )}
                      <label className="space-y-1.5">
                        <span className="text-sm font-semibold text-slate-600">Base URL</span>
                        <input
                          value={llmBaseUrl}
                          onChange={(event) => {
                            setLlmBaseUrl(event.target.value);
                            if (llmProvider.startsWith("custom")) setLlmModels([]);
                          }}
                          onBlur={() => {
                            if (!llmProvider.startsWith("custom") && llmProvider !== "opencode") loadLlmModels(llmProvider, llmBaseUrl, llmApiKey, { quiet: true });
                          }}
                          className="h-10 w-full rounded-lg border border-slate-200 px-3 text-base outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                        />
                      </label>
                      <label className="space-y-1.5">
                        <span className="flex items-center justify-between text-sm font-semibold text-slate-600">
                          <span>模型名称</span>
                          {llmModelsLoading && <span className="text-xs font-medium text-slate-400">加载中...</span>}
                        </span>
                        {profileLlmModels.length > 0 ? (
                          <select
                            value={llmModelName}
                            onChange={(event) => setLlmModelName(event.target.value)}
                            className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-base text-slate-900 outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                          >
                            {!llmModelName && <option value="">请选择模型</option>}
                            {profileLlmModels.map((model) => (
                              <option key={model.id} value={model.id}>
                                {model.name || model.id}{model.provider ? ` / ${model.provider}` : ""}
                              </option>
                            ))}
                          </select>
                        ) : (
                          <input value={llmModelName} onChange={(event) => setLlmModelName(event.target.value)} className="h-10 w-full rounded-lg border border-slate-200 px-3 text-base outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50" />
                        )}
                        {llmModelsNote && <p className="break-words text-xs leading-relaxed text-slate-400">{llmModelsNote}</p>}
                        {(llmProvider.startsWith("custom") || llmProvider === "opencode") && (
                          <button
                            type="button"
                            onClick={() => loadLlmModels(llmProvider, llmBaseUrl, llmApiKey, { force: true })}
                            disabled={llmModelsLoading || !llmBaseUrl.trim() || !llmApiKey.trim()}
                            className="flex h-9 w-full items-center justify-center rounded-lg border border-slate-200 bg-white text-sm font-semibold text-slate-600 transition-colors hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
                          >
                            {llmModelsLoading ? "正在获取模型..." : "根据 Base URL 和 API Key 获取模型"}
                          </button>
                        )}
                      </label>
                      <label className="space-y-1.5">
                        <span className="text-sm font-semibold text-slate-600">API Key</span>
                        <input
                          type="password"
                          value={llmApiKey}
                          onChange={(event) => setLlmApiKey(event.target.value)}
                          onBlur={() => {
                            if (llmApiKey.trim() && llmBaseUrl.trim()) loadLlmModels(llmProvider, llmBaseUrl, llmApiKey, { quiet: true });
                          }}
                          placeholder={profileInfo?.llm.api_key_configured ? `已配置：${profileInfo.llm.api_key_preview}，留空则不修改` : "输入 API Key"}
                          className="h-10 w-full rounded-lg border border-slate-200 px-3 text-base outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                        />
                      </label>
                      <button
                        onClick={saveProfileLlmConfig}
                        disabled={profileSaving}
                        className="mt-1 flex h-10 items-center justify-center gap-2 rounded-lg bg-indigo-600 px-4 text-base font-semibold text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {profileSaving ? <Loader2 size={17} className="animate-spin" /> : <Save size={17} />}
                        保存配置
                      </button>
                      <p className="text-xs leading-relaxed text-slate-400">
                        切换供应商、Base URL、模型或 API Key 只会修改草稿；点击保存配置后才会成为智能体对话使用的模型。
                      </p>
                    </div>
                  </section>
                  <section className="min-w-[320px] space-y-4">
                    <div className="rounded-xl border border-slate-200 bg-slate-50 p-4">
                      <div className="mb-3 flex items-center gap-2 text-base font-bold text-slate-900">
                        <CircleUserRound size={20} className="text-slate-600" />
                        账户
                      </div>
                      <div className="space-y-2 text-base text-slate-700">
                        <div className="flex justify-between"><span>用户名</span><span className="font-semibold">{profileUsername}</span></div>
                        <div className="flex justify-between"><span>角色</span><span className="font-semibold">{profileInfo?.role ?? "管理员"}</span></div>
                      </div>
                    </div>

                    <div className={`rounded-xl border p-4 ${
                      labCardTone === "emerald"
                        ? "border-slate-200 bg-slate-50"
                        : labCardTone === "amber"
                          ? "border-amber-200 bg-amber-50"
                          : labCardTone === "red"
                            ? "border-red-200 bg-red-50"
                            : "border-slate-200 bg-slate-50"
                    }`}>
                      <div className="mb-3 flex items-center justify-between gap-2">
                        <div className={`flex items-center gap-2 text-base font-bold ${
                          labCardTone === "emerald"
                            ? "text-slate-900"
                            : labCardTone === "amber"
                              ? "text-amber-800"
                              : labCardTone === "red"
                                ? "text-red-800"
                                : "text-slate-800"
                        }`}>
                          <Network size={20} />
                          实验室计算服务
                        </div>
                        <span className={`rounded-full px-2.5 py-1 text-xs font-semibold ${
                          labOptimizationAvailable ? "bg-stone-100 text-stone-700" : "bg-red-100 text-red-700"
                        }`}>
                          {labOptimizationAvailable ? "预测与优化正常" : "预测与优化异常"}
                        </span>
                      </div>
                      <div className="mt-3 grid grid-cols-2 gap-2 text-xs font-semibold">
                        <div className={`rounded-lg border px-3 py-2 ${
                          labOptimizationAvailable
                            ? "border-stone-200 bg-white text-stone-700"
                            : "border-red-200 bg-white text-red-700"
                        }`}>
                          性能预测：{labOptimizationAvailable ? "正常" : "异常"}
                        </div>
                        <div className={`rounded-lg border px-3 py-2 ${
                          labOptimizationAvailable
                            ? "border-stone-200 bg-white text-stone-700"
                            : "border-amber-200 bg-white text-amber-700"
                        }`}>
                          工艺优化：{labOptimizationAvailable ? "正常" : "异常"}
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={handleLabServiceAction}
                        disabled={labServiceLoading}
                        className={`mt-3 flex h-9 w-full items-center justify-center gap-2 rounded-lg border bg-white text-sm font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                          labOptimizationAvailable
                            ? "border-stone-200 text-stone-700 hover:bg-stone-100"
                            : "border-red-200 text-red-700 hover:bg-red-100"
                        }`}
                      >
                        {labServiceLoading ? <Loader2 size={16} className="animate-spin" /> : <RefreshCw size={16} />}
                        重新检测
                      </button>
                    </div>

                    <div className="rounded-xl border border-slate-200 bg-slate-50 p-4">
                      <div className="mb-3 flex items-center gap-2 text-base font-bold text-slate-900">
                        <Gauge size={20} className="text-emerald-600" />
                        数据统计
                      </div>
                      <div className="grid grid-cols-2 gap-2 text-sm">
                        <div className="rounded-lg bg-white p-3"><div className="text-slate-400">文献</div><div className="text-lg font-bold text-slate-900">{profileInfo?.stats.literature_papers_count ?? "-"}</div></div>
                        <div className="rounded-lg bg-white p-3"><div className="text-slate-400">生产数据</div><div className="text-lg font-bold text-slate-900">{profileInfo?.stats.production_count ?? "-"}</div></div>
                        <div className="rounded-lg bg-white p-3"><div className="text-slate-400">文献配图</div><div className="text-lg font-bold text-slate-900">{profileInfo?.stats.literature_images_count ?? "-"}</div></div>
                        <div className="rounded-lg bg-white p-3"><div className="text-slate-400">金相照片</div><div className="text-lg font-bold text-slate-900">{profileInfo?.stats.experimental_images_count ?? "-"}</div></div>
                      </div>
                    </div>
                  </section>
                </div>

                {effectiveProfileTab === "registration" && (
                  <div className="space-y-4">
                    <section className="rounded-xl border border-slate-200 bg-white p-4">
                        <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
                          <div>
                            <div className="flex items-center gap-2 text-base font-bold text-slate-900">
                              <UserPlus size={20} className="text-indigo-600" />
                              注册设置
                            </div>
                            <p className="mt-1 text-xs leading-relaxed text-slate-500">
                              控制新用户注册、邮箱验证和忘记密码邮箱重置。
                            </p>
                          </div>
                        </div>

                        <div className="divide-y divide-slate-100 rounded-xl border border-slate-200">
                          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3">
                            <div className="min-w-0">
                              <div className="text-sm font-semibold text-slate-900">开放注册</div>
                              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">关闭后，新用户不能自行创建账号。</div>
                            </div>
                            <SettingsSwitch
                              label="开放注册"
                              checked={authSecurityConfig.registration_enabled}
                              onChange={(checked) => setAuthSecurityConfig((prev) => {
                                const next = { ...prev, registration_enabled: checked };
                                void saveAuthSecurityConfig(next);
                                return next;
                              })}
                            />
                          </div>
                          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3">
                            <div className="min-w-0">
                              <div className="text-sm font-semibold text-slate-900">邮箱验证</div>
                              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">开启后，注册时必须先通过邮箱验证码。</div>
                            </div>
                            <SettingsSwitch
                              label="邮箱验证"
                              checked={authSecurityConfig.email_verify_enabled}
                              onChange={(checked) => setAuthSecurityConfig((prev) => {
                                const next = {
                                  ...prev,
                                  email_verify_enabled: checked,
                                  password_reset_enabled: checked ? prev.password_reset_enabled : false,
                                };
                                void saveAuthSecurityConfig(next);
                                return next;
                              })}
                            />
                          </div>
                          {authSecurityConfig.email_verify_enabled && (
                            <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3">
                              <div className="min-w-0">
                                <div className="text-sm font-semibold text-slate-900">忘记密码邮箱重置</div>
                                <div className="mt-0.5 text-xs leading-relaxed text-slate-500">开启后，用户可以通过邮箱验证码重置密码。</div>
                              </div>
                              <SettingsSwitch
                                label="忘记密码邮箱重置"
                                checked={authSecurityConfig.password_reset_enabled}
                                onChange={(checked) => setAuthSecurityConfig((prev) => {
                                  const next = { ...prev, password_reset_enabled: checked };
                                  void saveAuthSecurityConfig(next);
                                  return next;
                                })}
                              />
                            </div>
                          )}
                          {authSecurityConfig.email_verify_enabled && authSecurityConfig.password_reset_enabled && (
                            <div className="px-4 py-3">
                              <label className="block space-y-1.5">
                                <span className="flex items-center gap-2 text-sm font-semibold text-slate-900">
                                  <Globe size={16} className="text-slate-500" />
                                  前端地址
                                </span>
                                <input
                                  value={authSecurityConfig.frontend_url}
                                  onChange={(event) => setAuthSecurityConfig((prev) => ({
                                    ...prev,
                                    frontend_url: event.target.value,
                                  }))}
                                  onBlur={() => void saveAuthSecurityConfig()}
                                  placeholder="https://your-domain.com"
                                  className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                                />
                                <span className="block text-xs leading-relaxed text-slate-400">
                                  用于邮件中的密码重置入口，请填写用户实际访问的网站地址。
                                </span>
                              </label>
                            </div>
                          )}
                        </div>
                      </section>
                  </div>
                )}

                {effectiveProfileTab === "knowledge" && (
                  <div className="space-y-4">
                    <section className="rounded-xl border border-slate-200 bg-white p-4">
                        <div className="mb-4 grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
                          <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-2 text-base font-bold text-slate-900">
                              <Globe size={20} className="text-indigo-600" />
                              <span>管理员知识库共享</span>
                              <span
                                className={`shrink-0 rounded-full px-2 py-0.5 text-xs font-semibold ${
                                  knowledgeBaseSecurityConfig.shared_enabled
                                    ? "bg-emerald-100 text-emerald-700"
                                    : "bg-slate-200 text-slate-500"
                                }`}
                              >
                                {knowledgeBaseSecurityConfig.shared_enabled ? "已开启" : "已关闭"}
                              </span>
                            </div>
                            <p className="mt-1 text-xs leading-relaxed text-slate-500">
                              关闭时，所有新建知识库默认只对创建者可见；开启后，管理员新建的知识库会作为共享知识库入库，普通用户仍然默认私有。
                            </p>
                          </div>
                            <SettingsSwitch
                            label={knowledgeBaseSecurityConfig.shared_enabled ? "关闭管理员知识库共享" : "开启管理员知识库共享"}
                            checked={knowledgeBaseSecurityConfig.shared_enabled}
                            onChange={(checked) => setKnowledgeBaseSecurityConfig((prev) => {
                              const next = { ...prev, shared_enabled: checked };
                              void saveKnowledgeBaseSecurityConfig(next);
                              return next;
                            })}
                          />
                        </div>
                      </section>
                  </div>
                )}

                {effectiveProfileTab === "mineru" && (
                  <div className="space-y-4">
                    <section className="rounded-xl border border-slate-200 bg-white p-4">
                        <div className="mb-4 grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
                          <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-2 text-base font-bold text-slate-900">
                              <KeyRound size={20} className="text-indigo-600" />
                              <span>文献解析后端</span>
                              <span className="shrink-0 rounded-full bg-slate-100 px-2 py-0.5 text-xs font-semibold text-slate-600">
                                {mineruProcessingModeLabel(mineruProcessingConfig.provider_mode)}
                              </span>
                            </div>
                            <p className="mt-1 text-xs leading-relaxed text-slate-500">
                              文献解析统一调用 MinerU 官方线上 API。API Key 只会加密保存，不会在前端回显。
                            </p>
                          </div>
                        </div>

                        <div className="grid gap-4 md:grid-cols-2">
                          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2.5 text-sm">
                            <div className="font-semibold text-slate-700">处理后端</div>
                            <div className="mt-2 flex h-10 items-center rounded-lg border border-slate-200 bg-white px-3 text-slate-900">
                              MinerU 官方云端 API
                            </div>
                          </div>
                          <label className="block text-sm font-semibold text-slate-700">
                            API 地址
                            <input
                              value={mineruProcessingConfig.api_base}
                              onChange={(event) => setMineruProcessingConfig((prev) => ({
                                ...prev,
                                api_base: event.target.value,
                              }))}
                              onBlur={() => void saveMineruProcessingConfig()}
                              placeholder="https://mineru.net"
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            />
                          </label>
                          <label className="block text-sm font-semibold text-slate-700">
                            模型版本
                            <select
                              value={mineruProcessingConfig.model_version}
                              onChange={(event) => setMineruProcessingConfig((prev) => {
                                const next = { ...prev, model_version: event.target.value };
                                void saveMineruProcessingConfig(next);
                                return next;
                              })}
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            >
                              <option value="vlm">vlm</option>
                              <option value="pipeline">pipeline</option>
                            </select>
                          </label>
                          <label className="block text-sm font-semibold text-slate-700">
                            批量文件数
                            <input
                              type="number"
                              min={1}
                              max={200}
                              value={mineruProcessingConfig.batch_size}
                              onChange={(event) => setMineruProcessingConfig((prev) => ({
                                ...prev,
                                batch_size: Math.max(1, Math.min(200, Number(event.target.value) || 1)),
                              }))}
                              onBlur={() => void saveMineruProcessingConfig()}
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            />
                          </label>
                          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-slate-200 bg-slate-50 px-3 py-3">
                            <div className="min-w-0">
                              <div className="text-sm font-semibold text-slate-800">默认启用 OCR</div>
                              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">图片型 PDF 或扫描页可识别文字。</div>
                            </div>
                            <SettingsSwitch
                              label="默认启用 OCR"
                              checked={mineruProcessingConfig.file_is_ocr !== false}
                              onChange={(checked) => setMineruProcessingConfig((prev) => {
                                const next = { ...prev, file_is_ocr: checked };
                                void saveMineruProcessingConfig(next);
                                return next;
                              })}
                            />
                          </div>
                          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-slate-200 bg-slate-50 px-3 py-3">
                            <div className="min-w-0">
                              <div className="text-sm font-semibold text-slate-800">默认识别公式</div>
                              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">保留论文中的行内公式和公式内容。</div>
                            </div>
                            <SettingsSwitch
                              label="默认识别公式"
                              checked={mineruProcessingConfig.enable_formula !== false}
                              onChange={(checked) => setMineruProcessingConfig((prev) => {
                                const next = { ...prev, enable_formula: checked };
                                void saveMineruProcessingConfig(next);
                                return next;
                              })}
                            />
                          </div>
                          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-slate-200 bg-slate-50 px-3 py-3">
                            <div className="min-w-0">
                              <div className="text-sm font-semibold text-slate-800">默认识别表格</div>
                              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">解析表格结构，供后续 Markdown 和入库使用。</div>
                            </div>
                            <SettingsSwitch
                              label="默认识别表格"
                              checked={mineruProcessingConfig.enable_table !== false}
                              onChange={(checked) => setMineruProcessingConfig((prev) => {
                                const next = { ...prev, enable_table: checked };
                                void saveMineruProcessingConfig(next);
                                return next;
                              })}
                            />
                          </div>
                          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-3 text-sm leading-6 text-slate-600">
                            <div className="font-semibold text-slate-800">线上服务限制</div>
                            <div>单文件 ≤ {mineruUsage?.limits?.max_file_size_mb ?? 200}MB</div>
                            <div>单文件 ≤ {mineruUsage?.limits?.max_pages ?? 200} 页</div>
                            <div>单批次 ≤ {mineruUsage?.limits?.max_batch_files ?? 200} 个文件</div>
                          </div>
                          <label className="block text-sm font-semibold text-slate-700 md:col-span-2">
                            MinerU API Key
                            <input
                              value={mineruApiKey}
                              onChange={(event) => setMineruApiKey(event.target.value)}
                              onBlur={() => {
                                if (mineruApiKey.trim()) void saveMineruProcessingConfig(undefined, mineruApiKey);
                              }}
                              type="password"
                              placeholder={mineruProcessingConfig.api_key_configured ? "留空则保留已保存的 API Key" : "请输入 MinerU API Key"}
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            />
                            <span className="mt-2 block text-xs leading-relaxed text-slate-400">
                              {mineruProcessingConfig.api_key_configured
                                ? `已配置：${mineruProcessingConfig.api_key_preview || "******"}`
                                : "未配置 API Key，请先填写后再提交解析任务。"}
                            </span>
                          </label>
                        </div>

                      </section>
                  </div>
                )}

                {effectiveProfileTab === "retrieval" && (
                  <div className="space-y-4">
                    <section className="rounded-xl border border-slate-200 bg-white p-4">
                        <div className="mb-4 grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
                          <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-2 text-base font-bold text-slate-900">
                              <KeyRound size={20} className="text-indigo-600" />
                              <span>检索模型后端</span>
                              <span className="shrink-0 rounded-full bg-slate-100 px-2 py-0.5 text-xs font-semibold text-slate-600">
                                {retrievalModeLabel(retrievalModelsConfig.provider_mode)}
                              </span>
                            </div>
                            <p className="mt-1 text-xs leading-relaxed text-slate-500">
                              文献检索统一使用硅基流动云端 API。API Key 只会加密保存，不会在前端回显。
                            </p>
                          </div>
                        </div>

                        <div className="grid gap-4 md:grid-cols-2">
                          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2.5 text-sm">
                            <div className="font-semibold text-slate-700">检索后端</div>
                            <div className="mt-2 flex h-10 items-center rounded-lg border border-slate-200 bg-white px-3 text-slate-900">
                              硅基流动云端 API
                            </div>
                          </div>
                          <label className="block text-sm font-semibold text-slate-700">
                            API 地址
                            <input
                              value={retrievalModelsConfig.api_base}
                              onChange={(event) => setRetrievalModelsConfig((prev) => ({
                                ...prev,
                                api_base: event.target.value,
                              }))}
                              onBlur={() => void saveRetrievalModelsConfig()}
                              placeholder="https://api.siliconflow.cn"
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            />
                          </label>
                          <label className="block text-sm font-semibold text-slate-700">
                            Embedding 模型
                            <select
                              value={retrievalModelsConfig.embedding_model}
                              onChange={(event) => setRetrievalModelsConfig((prev) => { const next = { ...prev, embedding_model: event.target.value }; void saveRetrievalModelsConfig(next); return next; })}
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            >
                              <option value="BAAI/bge-m3">免费版 · BGE-M3</option>
                              <option value="Pro/BAAI/bge-m3">Pro 版 · BGE-M3</option>
                            </select>
                            <span className="mt-2 block text-xs font-normal leading-relaxed text-slate-400">
                              免费版固定限流；Pro 版按量计费，可随账户用量等级提升限额。
                            </span>
                          </label>
                          <label className="block text-sm font-semibold text-slate-700">
                            Rerank 模型
                            <select
                              value={retrievalModelsConfig.rerank_model}
                              onChange={(event) => setRetrievalModelsConfig((prev) => { const next = { ...prev, rerank_model: event.target.value }; void saveRetrievalModelsConfig(next); return next; })}
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            >
                              <option value="BAAI/bge-reranker-v2-m3">免费版 · BGE Reranker v2 M3</option>
                              <option value="Pro/BAAI/bge-reranker-v2-m3">Pro 版 · BGE Reranker v2 M3</option>
                            </select>
                            <span className="mt-2 block text-xs font-normal leading-relaxed text-slate-400">
                              两个模型档位可独立选择；仅带 Pro/ 前缀的模型产生费用。
                            </span>
                          </label>
                          <label className="block text-sm font-semibold text-slate-700 md:col-span-2">
                            硅基流动 API Key
                            <input
                              value={retrievalApiKey}
                              onChange={(event) => setRetrievalApiKey(event.target.value)}
                              onBlur={() => {
                                if (retrievalApiKey.trim()) void saveRetrievalModelsConfig(undefined, retrievalApiKey);
                              }}
                              type="password"
                              placeholder={retrievalModelsConfig.api_key_configured ? "留空则保留已保存的 API Key" : "请输入硅基流动 API Key"}
                              className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50"
                            />
                            <span className="mt-2 block text-xs leading-relaxed text-slate-400">
                              {retrievalModelsConfig.api_key_configured
                                ? `已配置：${retrievalModelsConfig.api_key_preview || "******"}`
                                : "未配置 API Key，请先填写后再执行文献检索。"}
                            </span>
                          </label>
                        </div>
                      </section>
                  </div>
                )}

                {effectiveProfileTab === "captcha" && (
                  <div className="space-y-4">
                    <section
                        className={`rounded-xl border p-4 transition-colors ${
                          captchaProvider !== "none"
                            ? "border-cyan-200 bg-cyan-50/60"
                            : "border-slate-200 bg-white"
                        }`}
                      >
                        <div className="mb-4 grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
                          <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-2 text-base font-bold text-slate-900">
                              <ShieldCheck
                                size={20}
                                className={`shrink-0 ${captchaProvider !== "none" ? "text-cyan-600" : "text-slate-400"}`}
                              />
                              <span>人机验证</span>
                              {turnstileSaving && <Loader2 size={16} className="shrink-0 animate-spin text-cyan-500" />}
                            </div>
                            <p className="mt-1 text-xs leading-relaxed text-slate-500">
                              登录、注册、发送验证码和找回密码前的人机验证防护，四选一。
                            </p>
                          </div>
                        </div>

                        <label className="mb-3 block space-y-1.5">
                          <span className="text-sm font-semibold text-slate-600">验证方式</span>
                          <select
                            value={captchaProvider}
                            onChange={(event) => {
                              const provider = event.target.value as CaptchaProviderValue;
                              setCaptchaProvider(provider);
                              void saveCaptchaAdminConfig({ provider });
                            }}
                            className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-cyan-300 focus:ring-4 focus:ring-cyan-50"
                          >
                            <option value="none">关闭（不验证）</option>
                            <option value="turnstile">Cloudflare Turnstile</option>
                            <option value="geetest">极验滑块（Geetest）</option>
                            <option value="slider">内置滑块</option>
                          </select>
                        </label>

                        {captchaProvider === "turnstile" && (
                          <div className="grid gap-3">
                            <label className="space-y-1.5">
                              <span className="text-sm font-semibold text-slate-600">站点密钥</span>
                              <input
                                value={turnstileSiteKey}
                                onChange={(event) => setTurnstileSiteKey(event.target.value)}
                                onBlur={() => void saveCaptchaAdminConfig()}
                                placeholder="0x4AAAA..."
                                className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-cyan-300 focus:ring-4 focus:ring-cyan-50"
                              />
                              <span className="block text-xs leading-relaxed text-slate-400">
                                从 Cloudflare Dashboard 获取，前端会公开使用这个 site key。
                              </span>
                            </label>
                            <label className="space-y-1.5">
                              <span className="flex items-center gap-2 text-sm font-semibold text-slate-600">
                                <Mail size={15} className="text-slate-400" />
                                私密密钥
                              </span>
                              <input
                                type="password"
                                value={turnstileSecretKey}
                                onChange={(event) => setTurnstileSecretKey(event.target.value)}
                                onBlur={() => {
                                  if (turnstileSecretKey.trim()) void saveCaptchaAdminConfig();
                                }}
                                placeholder={captchaConfig?.turnstile_secret_key_configured ? `已配置：${captchaConfig.turnstile_secret_key_preview}，留空则保留当前值` : "输入 Secret Key"}
                                className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-cyan-300 focus:ring-4 focus:ring-cyan-50"
                              />
                              <span className="block text-xs leading-relaxed text-slate-400">
                                私密密钥只保存在后端数据库中，不会返回给前端。
                              </span>
                            </label>
                          </div>
                        )}

                        {captchaProvider === "geetest" && (
                          <div className="grid gap-3">
                            <label className="space-y-1.5">
                              <span className="text-sm font-semibold text-slate-600">极验 ID（captcha_id）</span>
                              <input
                                value={geetestCaptchaId}
                                onChange={(event) => setGeetestCaptchaId(event.target.value)}
                                onBlur={() => void saveCaptchaAdminConfig()}
                                placeholder="极验后台获取的 captcha_id"
                                className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-cyan-300 focus:ring-4 focus:ring-cyan-50"
                              />
                              <span className="block text-xs leading-relaxed text-slate-400">
                                极验官网「行为验证」应用的 ID，前端会公开使用。
                              </span>
                            </label>
                            <label className="space-y-1.5">
                              <span className="flex items-center gap-2 text-sm font-semibold text-slate-600">
                                <KeyRound size={15} className="text-slate-400" />
                                极验 KEY（private_key）
                              </span>
                              <input
                                type="password"
                                value={geetestPrivateKey}
                                onChange={(event) => setGeetestPrivateKey(event.target.value)}
                                onBlur={() => {
                                  if (geetestPrivateKey.trim()) void saveCaptchaAdminConfig();
                                }}
                                placeholder={captchaConfig?.geetest_private_key_configured ? `已配置：${captchaConfig.geetest_private_key_preview}，留空则保留当前值` : "输入极验 KEY"}
                                className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-cyan-300 focus:ring-4 focus:ring-cyan-50"
                              />
                              <span className="block text-xs leading-relaxed text-slate-400">
                                极验 KEY 只保存在后端数据库中，不会返回给前端。
                              </span>
                            </label>
                          </div>
                        )}

                        {captchaProvider === "slider" && (
                          <p className="rounded-lg bg-slate-50 px-3 py-2 text-xs leading-relaxed text-slate-500">
                            使用系统内置滑块验证，无需额外密钥配置。
                          </p>
                        )}
                        {captchaProvider === "none" && (
                          <p className="rounded-lg bg-slate-50 px-3 py-2 text-xs leading-relaxed text-slate-500">
                            已关闭人机验证，登录/注册/发码将不再要求验证。
                          </p>
                        )}
                      </section>
                  </div>
                )}

                {effectiveProfileTab === "asr" && (
                  <div className="space-y-4">
                    <section className="rounded-xl border border-slate-200 bg-white p-4">
                      <div className="mb-4">
                        <div className="flex flex-wrap items-center gap-2 text-base font-bold text-slate-900">
                          <span>语音输入（讯飞听写）</span>
                        </div>
                        <p className="mt-1 text-xs leading-relaxed text-slate-500">
                          配置讯飞开放平台的语音听写（IAT）凭证，用于对话输入框的麦克风实时语音转文字。凭证只保存在后端数据库中，前端仅显示脱敏预览。
                        </p>
                      </div>
                      <AsrConfigManager />
                    </section>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

function mineruProcessingModeLabel(mode: MinerUProcessingConfigInfo["provider_mode"]) {
  return "云端 API";
}

function retrievalModeLabel(mode: RetrievalModelsConfigInfo["provider_mode"]) {
  return "云端 API";
}
