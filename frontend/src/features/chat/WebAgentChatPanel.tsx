import { useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import {
  Activity,
  ArrowUp,
  Check,
  ChevronDown,
  Clock3,
  Copy,
  Download,
  FileJson,
  Globe,
  LoaderCircle,
  Mic,
  Pencil,
  Send,
  ShieldAlert,
  Sparkles,
  Square,
  Wrench,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import type {
  ConversationExportFormat,
  EvidenceItem,
  LocalAgentAttachment,
  Message,
  ProviderProfileResponse,
} from "../../bridge/desktop";
import type { AgentRunState, PermissionDecision } from "../../bridge/generated/protocol";
import type { ChatControllerProps } from "./chatController";
import CitationPanel from "./CitationPanel";
import type { AgentPermissionView, AgentRunView } from "./agentEvents";
import WebAnswerRenderer from "./web/WebAnswerRenderer";
import WebConfirmDialog from "./web/WebConfirmDialog";
import WebFeedback from "./web/WebFeedback";
import WebProgressBar, { progressFromRunState } from "./web/WebProgressBar";
import WebRecommendationCard from "./web/WebRecommendationCard";
import WebTurnNavigator from "./web/WebTurnNavigator";
import { toWebMessage, type WebPendingConfirmation, type WebResponse } from "./web/webTypes";

interface SpeechRecognitionLike {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  start: () => void;
  stop: () => void;
  onresult: ((event: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
  onerror: ((event: { error?: string }) => void) | null;
  onend: (() => void) | null;
}

export interface WebAgentChatPanelProps extends ChatControllerProps {
  showConversationSidebar?: boolean;
}

function isAssistant(message: Message) {
  return message.role === "agent" || message.role === "assistant";
}

function parseResponse(message: Message): Record<string, unknown> | null {
  if (!message.response_json || !isAssistant(message)) return null;
  try {
    const value: unknown = JSON.parse(message.response_json);
    return value && typeof value === "object" ? value as Record<string, unknown> : null;
  } catch {
    return null;
  }
}

function responseEvidence(message: Message): { auditId: string; evidence: EvidenceItem[] } | null {
  const response = parseResponse(message);
  if (!response || typeof response.evidence_pack_id !== "string" || !Array.isArray(response.evidence)) return null;
  return { auditId: response.evidence_pack_id, evidence: response.evidence as EvidenceItem[] };
}

function copyText(content: string) {
  if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(content);
  const textarea = document.createElement("textarea");
  textarea.value = content;
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  textarea.remove();
  return Promise.resolve();
}

function stateLabel(
  state: AgentRunState,
  translate: (key: "contextPreparing" | "generating" | "runtimeReady" | "stopGenerating" | "chatError") => string,
) {
  switch (state) {
    case "created":
    case "preparing":
    case "awaiting_permission":
      return translate("contextPreparing");
    case "generating":
    case "executing_tools":
    case "verifying":
    case "completing":
      return translate("generating");
    case "completed":
      return translate("runtimeReady");
    case "cancelled":
      return translate("stopGenerating");
    case "failed":
      return translate("chatError");
    case "interrupted":
      return translate("contextPreparing");
  }
}

function stateTone(state: string) {
  if (state === "completed") return "is-good";
  if (state === "failed" || state === "cancelled" || state === "interrupted") return "is-danger";
  return "is-pending";
}

function WebPermissionPanel({
  permissions,
  onResolvePermission,
}: {
  permissions: AgentPermissionView[];
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const { t } = useLocale();
  const pending = permissions.filter((permission) => permission.decision === null);
  if (pending.length === 0) return null;

  const actions: Array<{ decision: PermissionDecision; label: string; ariaLabel: string; icon: typeof Check }> = [
    { decision: "allow_once", label: t("allowOnce"), ariaLabel: "Allow once", icon: Check },
    { decision: "allow_session", label: t("allowSession"), ariaLabel: "Allow for session", icon: Check },
    { decision: "allow_always", label: t("allowAlways"), ariaLabel: "Always allow", icon: Check },
    { decision: "deny", label: t("deny"), ariaLabel: "Deny", icon: X },
  ];

  return (
    <div className="mt-4 rounded-2xl border border-amber-200 bg-amber-50 p-4 shadow-sm" role="alert">
      <div className="mb-3 flex items-center gap-2 text-base font-semibold text-amber-800">
        <ShieldAlert size={17} aria-hidden="true" />
        {t("permissionRequired")}
      </div>
      <div className="space-y-2">
        {pending.map((permission) => (
          <div key={permission.permissionId} className="rounded-xl border border-amber-200 bg-white p-3">
            <strong className="block text-sm text-slate-900">{permission.summary}</strong>
            <p className="mt-1 text-sm leading-relaxed text-amber-800">{permission.reason}</p>
            <div className="mt-3 flex flex-wrap gap-2">
              {actions.map(({ decision, label, ariaLabel, icon: Icon }) => (
                <button
                  key={decision}
                  type="button"
                  className={decision === "deny"
                    ? "flex items-center gap-1.5 rounded-full border border-red-200 px-3 py-1.5 text-sm text-red-700 hover:bg-red-50"
                    : "flex items-center gap-1.5 rounded-full bg-amber-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-amber-700"}
                  aria-label={`${ariaLabel} / ${label}`}
                  onClick={() => onResolvePermission(permission.permissionId, decision)}
                >
                  <Icon size={14} aria-hidden="true" />
                  {label}
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function WebRunStatus({
  agentRun,
  onResolvePermission,
}: {
  agentRun: AgentRunView | null;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const { t } = useLocale();
  if (!agentRun) return null;
  const pending = agentRun.permissions.some((permission) => permission.decision === null);
  const settled = ["completed", "cancelled", "failed", "interrupted"].includes(agentRun.state);
  if (settled && !pending) return null;

  return (
    <div className="mx-auto w-full max-w-5xl px-2 pb-3" aria-live="polite">
      <div className="flex flex-wrap items-center gap-3 text-xs text-slate-500">
        <span className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-medium ${
          stateTone(agentRun.state) === "is-good"
            ? "border-emerald-200 bg-emerald-50 text-emerald-700"
            : stateTone(agentRun.state) === "is-danger"
              ? "border-red-200 bg-red-50 text-red-700"
              : "border-amber-200 bg-amber-50 text-amber-700"
        }`}>
          <Activity size={13} aria-hidden="true" />
          {stateLabel(agentRun.state, t)}
        </span>
        {agentRun.toolCalls.length > 0 && (
          <span className="inline-flex items-center gap-1.5">
            <Wrench size={13} aria-hidden="true" />
            {t("agentToolCount", { count: agentRun.toolCalls.length })}
          </span>
        )}
        {agentRun.taskProgress && (
          <span className="inline-flex items-center gap-1.5">
            <Clock3 size={13} aria-hidden="true" />
            {agentRun.taskProgress.progress}%
          </span>
        )}
      </div>
      <WebPermissionPanel permissions={agentRun.permissions} onResolvePermission={onResolvePermission} />
    </div>
  );
}

function WebContextStatus({ response }: { response: WebResponse | null }) {
  const { t } = useLocale();
  const status = response?.context_status;
  if (!status || (status.memory_count === 0 && status.skill_count === 0 && status.tool_count === 0)) {
    return null;
  }
  return (
    <div className="bloomery-chat-context-status" aria-label={t("chatContextStatus")}>
      {status.memory_count > 0 && <span>{t("chatContextMemoryCount", { count: status.memory_count })}</span>}
      {status.skill_count > 0 && <span>{t("chatContextSkillCount", { count: status.skill_count })}</span>}
      {status.tool_count > 0 && <span>{t("chatContextToolCount", { count: status.tool_count })}</span>}
    </div>
  );
}

function WebMessage({
  message,
  index,
  loading,
  onEdit,
  onSelectFollowUp,
  onResolvePermission,
}: {
  message: Message;
  index: number;
  loading: boolean;
  onEdit: () => void;
  onSelectFollowUp: (question: string) => void;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const [copied, setCopied] = useState(false);
  const handleCopy = async () => {
    await copyText(message.content);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };

  if (!isAssistant(message)) {
    return (
      <div className="group flex justify-end" data-agent-user-turn={index}>
        <div className="group/message flex max-w-[60%] flex-col items-end max-md:max-w-[86%]">
          <div className="rounded-2xl rounded-tr-md bg-[#4f382a] px-4 py-3 text-base leading-relaxed text-white shadow-sm">
            <div className="whitespace-pre-wrap break-words">{message.content}</div>
          </div>
          <div className="mt-1 flex h-8 items-center justify-end gap-1 opacity-0 transition-opacity duration-100 group-hover/message:opacity-100 focus-within:opacity-100">
            <button
              type="button"
              onClick={() => void handleCopy()}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 hover:bg-slate-100 hover:text-slate-900"
              aria-label={copied ? "已复制" : "复制消息"}
              title={copied ? "已复制" : "复制消息"}
            >
              {copied ? <Check size={17} /> : <Copy size={17} />}
            </button>
            <button
              type="button"
              onClick={onEdit}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 hover:bg-slate-100 hover:text-slate-900"
              aria-label="编辑消息"
              title="编辑消息"
            >
              <Pencil size={17} />
            </button>
          </div>
        </div>
      </div>
    );
  }

  const evidence = responseEvidence(message);
  const webMessage = toWebMessage(message);
  const response = webMessage.response;
  return (
    <article className="px-2 py-4" aria-label="Bloomery">
      <div className="ai-markdown-body w-full text-[17px] leading-relaxed text-slate-700">
        <WebAnswerRenderer message={webMessage} />
      </div>
      <WebContextStatus response={response} />
      {evidence && <CitationPanel auditId={evidence.auditId} evidence={evidence.evidence} />}
      {response?.follow_up_questions.length ? (
        <div className="mt-4 rounded-xl border border-blue-200 bg-blue-50 p-4 shadow-sm">
          <div className="mb-2 flex items-center gap-2 text-base font-semibold text-blue-800">
            <Sparkles size={16} aria-hidden="true" />
            需要补充的信息
          </div>
          <div className="grid gap-2 md:grid-cols-2">
            {response.follow_up_questions.map((question) => (
              <button
                key={question}
                type="button"
                className="rounded-lg border border-blue-200 bg-white px-3 py-2 text-left text-base leading-relaxed text-blue-700 hover:bg-blue-50"
                onClick={() => onSelectFollowUp(question)}
              >
                {question}
              </button>
            ))}
          </div>
        </div>
      ) : null}
      {response?.recommendations.length ? (
        <div className="mt-4">
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold uppercase tracking-widest text-slate-500">
            <Sparkles size={16} aria-hidden="true" />
            推荐方案
          </div>
          <div className="grid gap-3 xl:grid-cols-2">
            {response.recommendations.map((item, itemIndex) => (
              <WebRecommendationCard key={`${item.title}-${itemIndex}`} item={item} />
            ))}
          </div>
        </div>
      ) : null}
      {response?.pending_confirmations.length ? (
        <div className="mt-4">
          <WebConfirmDialog
            confirmations={response.pending_confirmations}
            onConfirm={(item: WebPendingConfirmation, approved) => onResolvePermission(
              item.action_id,
              approved ? "allow_once" : "deny",
            )}
          />
        </div>
      ) : null}
      {!loading && <WebFeedback messageId={message.id} />}
      <div className="mt-1 flex h-8 items-center gap-1 opacity-0 transition-opacity duration-100 hover:opacity-100 focus-within:opacity-100">
        <button
          type="button"
          onClick={() => void copyText(message.content)}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 hover:bg-slate-100 hover:text-slate-900"
          aria-label="复制回答"
          title="复制回答"
        >
          <Copy size={17} />
        </button>
      </div>
    </article>
  );
}

export default function WebAgentChatPanel({
  selectedId,
  selectedConversation,
  messages,
  loadingMessages,
  draft,
  pendingQuestion,
  agentRun,
  chatProfiles,
  activeChatProfileId,
  error,
  notice,
  smartSearchEnabled,
  attachments,
  onDraftChange,
  onAttachmentsChange,
  onSubmit,
  onCancel,
  onResolvePermission,
  onExportConversation,
  onSelectChatProfile,
  onToggleSmartSearch,
}: WebAgentChatPanelProps) {
  const { t } = useLocale();
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const messagesRef = useRef<HTMLDivElement>(null);
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const [speechRecording, setSpeechRecording] = useState(false);
  const [speechError, setSpeechError] = useState<string | null>(null);
  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);
  const activeRun = agentRun && agentRun.conversationId === selectedId ? agentRun : null;
  const webMessages = useMemo(() => messages.map(toWebMessage), [messages]);
  const activeProfile = chatProfiles.find((profile) => profile.id === activeChatProfileId) ?? null;
  const selectedModelLabel = activeProfile?.model_id || activeProfile?.display_name || "本地模型";

  useEffect(() => () => recognitionRef.current?.stop(), []);

  const readImageAttachments = async (files: FileList | File[]) => {
    const images = Array.from(files).filter((file) => file.type.startsWith("image/"));
    if (images.length === 0) return;
    const results = (await Promise.all(images.map((file) => new Promise<LocalAgentAttachment | null>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = String(reader.result || "");
        const data = result.includes(",") ? result.slice(result.indexOf(",") + 1) : result;
        resolve(data ? { data, mime: file.type || "image/png", name: file.name || "image" } : null);
      };
      reader.onerror = () => resolve(null);
      reader.readAsDataURL(file);
    })))).filter((item): item is LocalAgentAttachment => item !== null);
    if (results.length > 0) onAttachmentsChange([...attachments, ...results]);
  };

  const removeAttachment = (index: number) => {
    onAttachmentsChange(attachments.filter((_, current) => current !== index));
  };

  const toggleSpeech = () => {
    if (speechRecording) {
      recognitionRef.current?.stop();
      setSpeechRecording(false);
      return;
    }
    const browserWindow = window as typeof window & {
      SpeechRecognition?: new () => SpeechRecognitionLike;
      webkitSpeechRecognition?: new () => SpeechRecognitionLike;
    };
    const Recognition = browserWindow.SpeechRecognition ?? browserWindow.webkitSpeechRecognition;
    if (!Recognition) {
      setSpeechError("当前 Windows 环境未提供浏览器语音识别");
      return;
    }
    const recognition = new Recognition();
    recognition.lang = "zh-CN";
    recognition.continuous = false;
    recognition.interimResults = true;
    recognition.onresult = (event) => {
      const text = Array.from({ length: event.results.length }, (_, index) => event.results[index]?.[0]?.transcript ?? "").join("");
      if (text) onDraftChange(text);
    };
    recognition.onerror = (event) => {
      setSpeechError(event.error || "语音识别失败");
      setSpeechRecording(false);
    };
    recognition.onend = () => setSpeechRecording(false);
    recognitionRef.current = recognition;
    setSpeechError(null);
    setSpeechRecording(true);
    recognition.start();
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSubmit(event);
  };

  const handleComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      event.currentTarget.form?.requestSubmit();
    }
  };

  return (
    <section
      className="web-agent-chat-panel bloomery-chat is-embedded flex min-h-0 flex-1 overflow-hidden bg-[#fbf7ef]"
      aria-label="Web 风格对话面板"
    >
      <div className="flex h-full min-h-0 flex-1 flex-col">
        <header className="shrink-0 border-b border-[#eadfd2] bg-[#fbf7ef]/95 px-6 py-3 max-md:px-3">
          {selectedConversation && (
            <div className="flex items-center justify-end gap-1" aria-label={t("chatExportActions")}>
              <button
                type="button"
                className="bloomery-icon-button"
                data-testid="export-conversation-markdown"
                onClick={() => onExportConversation("markdown" satisfies ConversationExportFormat)}
                aria-label={t("chatExportMarkdown")}
                title={t("chatExportMarkdown")}
              >
                <Download size={17} aria-hidden="true" />
              </button>
              <button
                type="button"
                className="bloomery-icon-button"
                data-testid="export-conversation-json"
                onClick={() => onExportConversation("json" satisfies ConversationExportFormat)}
                aria-label={t("chatExportJson")}
                title={t("chatExportJson")}
              >
                <FileJson size={17} aria-hidden="true" />
              </button>
            </div>
          )}
        </header>

        {error && <div className="mx-auto mt-3 w-full max-w-5xl px-2" role="alert"><div className="rounded-xl border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</div></div>}
        {notice && <div className="mx-auto mt-3 w-full max-w-5xl px-2" role="status"><div className="rounded-xl border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-700">{notice}</div></div>}

        <div ref={messagesRef} data-testid="web-agent-message-rail" className="relative min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-6 pt-5 max-md:px-3" aria-live="polite">
          <div className="mx-auto grid min-h-full w-full max-w-5xl grid-cols-[clamp(88px,10vw,220px)_minmax(0,1fr)_clamp(88px,10vw,220px)] max-md:grid-cols-1">
            <div className="col-start-2 min-w-0 space-y-3 max-md:col-start-1">
              {loadingMessages ? (
                <div className="flex min-h-[360px] items-center justify-center text-sm text-slate-500"><LoaderCircle size={20} className="mr-2 animate-spin" />{t("loading")}</div>
              ) : messages.length === 0 && pendingQuestion === null ? (
                <div className="flex min-h-[360px] flex-col items-center justify-center text-center">
                  <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl border border-[#eadfd2] bg-[#fffaf3] shadow-[0_16px_34px_rgba(72,52,38,0.10)]">
                    <Sparkles size={28} className="text-[#cc785c]" />
                  </div>
                  <h3 className="mb-1 text-lg font-semibold text-[#2b2118]">{t("startSpecificQuestion")}</h3>
                </div>
              ) : (
                <>
                  {messages.map((message, index) => (
                    <WebMessage
                      key={message.id}
                      message={message}
                      index={index}
                      loading={pendingQuestion !== null}
                      onEdit={() => {
                        onDraftChange(message.content);
                        composerRef.current?.focus();
                      }}
                      onSelectFollowUp={(question) => {
                        onDraftChange(question);
                        composerRef.current?.focus();
                      }}
                      onResolvePermission={onResolvePermission}
                    />
                  ))}
                  {pendingQuestion && (
                    <>
                      <div className="group flex justify-end" data-agent-user-turn="pending">
                        <div className="max-w-[60%] rounded-2xl rounded-tr-md bg-[#4f382a] px-4 py-3 text-base leading-relaxed text-white shadow-sm max-md:max-w-[86%]">{pendingQuestion}</div>
                      </div>
                      <article className="px-2 py-4" aria-label="Bloomery">
                        <div className="ai-markdown-body w-full text-[17px] leading-relaxed text-slate-700">
                          <WebAnswerRenderer
                            message={{
                              role: "agent",
                              content: activeRun?.assistantText || t("contextPreparing"),
                              response: null,
                              streamEvidence: [],
                            }}
                          />
                          {activeRun?.assistantText && <span className="ai-typing-cursor" aria-hidden="true" />}
                        </div>
                        {activeRun && activeRun.toolCalls.length > 0 && (
                          <div className="mt-3 flex flex-wrap gap-2 text-xs text-slate-500" aria-label="Agent tools">
                            {activeRun.toolCalls.map((tool) => (
                              <span key={tool.toolCallId} className="rounded-full border border-[#e3d7ca] bg-[#fffaf3] px-2.5 py-1">
                                {tool.name} · {tool.progress}%
                              </span>
                            ))}
                          </div>
                        )}
                      </article>
                    </>
                  )}
                  {activeRun && pendingQuestion && (
                    <WebProgressBar progress={progressFromRunState(activeRun.state)} />
                  )}
                  <WebRunStatus agentRun={activeRun} onResolvePermission={onResolvePermission} />
                </>
              )}
            </div>
          </div>
          <WebTurnNavigator messages={webMessages} scrollContainerRef={messagesRef} />
        </div>

        <form data-testid="web-agent-composer" className="shrink-0 border-t border-[#eadfd2] bg-[#fbf7ef]/95 px-6 py-3 max-md:px-3 max-md:pb-[calc(0.75rem+env(safe-area-inset-bottom))]" onSubmit={submit}>
          <div className="mx-auto grid w-full max-w-5xl grid-cols-[clamp(88px,10vw,220px)_minmax(0,1fr)_clamp(88px,10vw,220px)] max-md:grid-cols-1">
            <div className="col-start-2 min-w-0 max-md:col-start-1">
              <div className="rounded-2xl border border-[#e3d7ca] bg-[#fffaf3] p-2 shadow-[0_12px_30px_rgba(72,52,38,0.08)] transition-all duration-200 focus-within:border-[#cc785c]/45 focus-within:ring-4 focus-within:ring-[#cc785c]/10">
                {attachments.length > 0 && (
                  <div className="mb-2 flex flex-wrap gap-2 px-1" aria-label="已添加图片">
                    {attachments.map((attachment, index) => (
                      <div
                        key={`${attachment.name}-${index}`}
                        className="group/attachment relative h-16 w-16 overflow-hidden rounded-lg border border-[#e3d7ca] bg-[#f7efe5]"
                      >
                        <img
                          src={`data:${attachment.mime};base64,${attachment.data}`}
                          alt={attachment.name}
                          className="h-full w-full object-cover"
                        />
                        <button
                          type="button"
                          onClick={() => removeAttachment(index)}
                          className="absolute right-0.5 top-0.5 flex h-5 w-5 items-center justify-center rounded-full bg-black/55 text-white opacity-0 transition-opacity group-hover/attachment:opacity-100 focus:opacity-100"
                          aria-label={`移除图片 ${attachment.name}`}
                          title={`移除图片 ${attachment.name}`}
                        >
                          <X size={12} strokeWidth={2.6} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
                <textarea
                  ref={composerRef}
                  value={draft}
                  onChange={(event) => onDraftChange(event.target.value)}
                  onKeyDown={handleComposerKeyDown}
                  onPaste={(event) => {
                    const files = event.clipboardData?.files;
                    if (files && Array.from(files).some((file) => file.type.startsWith("image/"))) {
                      event.preventDefault();
                      void readImageAttachments(files);
                    }
                  }}
                  aria-label={t("inputMessage")}
                  placeholder={t("askPlaceholder")}
                  rows={2}
                  disabled={pendingQuestion !== null}
                  className="max-h-32 min-h-[44px] w-full resize-none bg-transparent px-3 py-2 text-base leading-relaxed text-[#2b2118] outline-none placeholder:text-[#a39384]"
                />
                <div className="mt-1 flex items-center justify-between gap-2">
                  <div className="flex shrink-0 items-center gap-2">
                    <button
                      type="button"
                      onClick={onToggleSmartSearch}
                      disabled={pendingQuestion !== null}
                      aria-label="智能搜索"
                      aria-pressed={smartSearchEnabled}
                      title="使用本地知识库检索"
                      className={`flex h-9 shrink-0 items-center gap-1.5 rounded-full border px-3 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
                        smartSearchEnabled ? "border-[#cc785c] bg-[#cc785c]/25 text-[#a85434]" : "border-[#e3d7ca] bg-transparent text-[#6f6258] hover:bg-[#f7efe5]"
                      }`}
                    >
                      <Globe size={16} aria-hidden="true" />
                      <span className="max-md:hidden">智能搜索</span>
                    </button>
                    <button
                      type="button"
                      onClick={toggleSpeech}
                      disabled={pendingQuestion !== null}
                      aria-label="语音"
                      title={speechError || "语音输入"}
                      className={`flex h-9 shrink-0 items-center gap-1.5 rounded-full border px-3 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
                        speechRecording ? "animate-pulse border-[#cc785c] bg-[#cc785c] text-white" : "border-[#e3d7ca] bg-transparent text-[#6f6258] hover:bg-[#f7efe5]"
                      }`}
                    >
                      <Mic size={16} aria-hidden="true" />
                      <span className="max-md:hidden">{speechRecording ? "聆听中" : "语音"}</span>
                    </button>
                  </div>
                  <div className="relative flex shrink-0 items-center gap-2">
                    {modelMenuOpen && (
                      <div className="absolute bottom-12 right-12 z-30 w-[min(460px,calc(100vw-64px))] overflow-hidden rounded-2xl border border-[#e3d7ca] bg-[#fffaf3] py-2 text-[#2b2118] shadow-2xl shadow-[#4c3425]/15 max-md:right-0 max-md:w-[min(460px,calc(100vw-24px))]" role="menu">
                        <div className="max-h-[320px] overflow-y-auto px-1.5">
                          {chatProfiles.length === 0 ? (
                            <span className="block px-3 py-2 text-sm text-[#7d7065]">请先在设置中配置聊天模型</span>
                          ) : chatProfiles.map((profile: ProviderProfileResponse) => (
                            <button
                              key={profile.id}
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                setModelMenuOpen(false);
                                onSelectChatProfile(profile.id);
                              }}
                              className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors ${profile.id === activeChatProfileId ? "bg-[#f0e5da] text-[#2b2118]" : "hover:bg-[#f7efe5]"}`}
                            >
                              <span className="flex h-5 w-5 shrink-0 items-center justify-center">{profile.id === activeChatProfileId && <Check size={16} />}</span>
                              <span className="min-w-0 flex-1 break-all text-sm font-semibold">{profile.model_id || profile.display_name}</span>
                            </button>
                          ))}
                        </div>
                      </div>
                    )}
                    <button
                      type="button"
                      onClick={() => setModelMenuOpen((open) => !open)}
                      disabled={pendingQuestion !== null}
                      className="flex h-10 shrink-0 items-center justify-end gap-1.5 bg-transparent px-1 text-sm font-medium text-[#6f6258] hover:text-[#2b2118] disabled:opacity-70"
                      title="切换当前对话模型"
                      aria-label="切换当前对话模型"
                      aria-expanded={modelMenuOpen}
                    >
                      <span className="whitespace-nowrap text-right max-md:max-w-[110px] max-md:truncate">{selectedModelLabel}</span>
                      <ChevronDown size={15} className={modelMenuOpen ? "rotate-180" : ""} />
                    </button>
                    <button
                      type={pendingQuestion ? "button" : "submit"}
                      onClick={pendingQuestion ? onCancel : undefined}
                      disabled={!pendingQuestion && !draft.trim() && attachments.length === 0}
                      className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-full shadow-md transition-all duration-150 disabled:cursor-not-allowed disabled:opacity-50 ${pendingQuestion ? "bg-[#fffaf3] text-[#2b2118] ring-1 ring-[#e3d7ca]" : "bg-[#6f5a48] text-white hover:-translate-y-0.5 hover:bg-[#5d4939]"}`}
                      aria-label={pendingQuestion ? t("stopGenerating") : t("send")}
                      title={pendingQuestion ? t("stopGenerating") : `发送，当前模型：${selectedModelLabel}`}
                    >
                      {pendingQuestion ? <Square size={15} fill="currentColor" /> : <ArrowUp size={20} strokeWidth={2.6} />}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </form>
      </div>
    </section>
  );
}
