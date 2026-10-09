import { useEffect, useRef, useState, type ChangeEvent, type FormEvent, type KeyboardEvent } from "react";
import {
  Archive,
  ArrowUp,
  Check,
  ChevronDown,
  Copy,
  CornerDownLeft,
  Download,
  FileJson,
  FileText,
  Globe,
  LoaderCircle,
  MessageSquarePlus,
  MoreHorizontal,
  Pencil,
  Pin,
  PinOff,
  Play,
  RotateCcw,
  Search,
  ShieldAlert,
  Square,
  Trash2,
  Wrench,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import type { SectionId } from "../../app/navigation";
import type { Conversation, Message } from "../../bridge/desktop";
import type { PermissionDecision } from "../../bridge/generated/protocol";
import AIAnswerRenderer from "../../components/answer/AnswerRenderer";
import CitationPanel from "./CitationPanel";
import type { AgentPermissionView, AgentRunView } from "./agentEvents";
import type { ChatControllerProps } from "./chatController";
import AgentRunInspector from "./AgentRunInspector";
import { parseWebResponse, toWebMessage, type WebPendingConfirmation } from "./web/webTypes";
import WebConfirmDialog from "./web/WebConfirmDialog";
import WebFeedback from "./web/WebFeedback";
import WebRecommendationCard from "./web/WebRecommendationCard";
import WebTurnNavigator from "./web/WebTurnNavigator";
import { useAppearanceSettings } from "../../settings/appearance";
import { desktop } from "../../bridge/desktop";
import SunaLogo from "../../components/SunaLogo";

function isAssistant(message: Message) {
  return message.role === "agent" || message.role === "assistant";
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

function responseEvidence(message: Message) {
  const response = parseWebResponse(message);
  if (!response || response.evidence.length === 0) return null;
  try {
    const record = JSON.parse(message.response_json ?? "") as { evidence_pack_id?: unknown };
    return typeof record.evidence_pack_id === "string"
      ? { auditId: record.evidence_pack_id, evidence: response.evidence }
      : null;
  } catch {
    return null;
  }
}

function stateTone(state: string) {
  if (state === "completed") return "is-good";
  if (["failed", "cancelled", "interrupted"].includes(state)) return "is-danger";
  return "is-pending";
}

function stateLabel(
  state: AgentRunView["state"],
  t: (key: "contextPreparing" | "generating" | "runtimeReady" | "stopGenerating" | "chatError") => string,
) {
  if (state === "completed") return t("runtimeReady");
  if (["failed"].includes(state)) return t("chatError");
  if (["cancelled"].includes(state)) return t("stopGenerating");
  return ["created", "preparing", "awaiting_permission", "interrupted"].includes(state)
    ? t("contextPreparing")
    : t("generating");
}

function NativePermissionPanel({
  permissions,
  onResolve,
}: {
  permissions: AgentPermissionView[];
  onResolve: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const { t } = useLocale();
  const pending = permissions.filter((permission) => permission.decision === null);
  if (pending.length === 0) return null;
  const actions: Array<{ decision: PermissionDecision; label: string }> = [
    { decision: "allow_once", label: t("allowOnce") },
    { decision: "allow_session", label: t("allowSession") },
    { decision: "allow_always", label: t("allowAlways") },
    { decision: "deny", label: t("deny") },
  ];

  return (
    <div className="suna-chat-permission-list" role="alert">
      {pending.map((permission) => (
        <div className="suna-chat-permission" key={permission.permissionId}>
          <div className="suna-chat-permission-heading">
            <ShieldAlert size={16} aria-hidden="true" />
            <div>
              <strong>{t("permissionRequired")}</strong>
              <span>{permission.summary}</span>
            </div>
          </div>
          <p>{permission.reason}</p>
          <div className="suna-chat-permission-actions">
            {actions.map(({ decision, label }) => (
              <button
                type="button"
                className={decision === "deny" ? "suna-action-secondary" : "suna-action-primary"}
                key={decision}
                onClick={() => onResolve(permission.permissionId, decision)}
              >
                {decision === "deny" ? <X size={14} aria-hidden="true" /> : <Check size={14} aria-hidden="true" />}
                {label}
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function NativeRunStatus({
  run,
  recovery,
  onResolvePermission,
  onRetry,
  onResume,
}: {
  run: AgentRunView | null;
  recovery: ChatControllerProps["recovery"];
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
  onRetry: () => void;
  onResume: () => void;
}) {
  const { t } = useLocale();
  if (!run) return null;
  const hasPendingPermission = run.permissions.some((permission) => permission.decision === null);
  const settled = ["completed", "failed", "cancelled", "interrupted"].includes(run.state);
  const canRetry = settled && run.state !== "completed";
  const canResume = recovery?.action.kind === "resume_from_checkpoint";
  if (settled && !hasPendingPermission && run.toolCalls.length === 0 && !canRetry && !canResume) return null;

  return (
    <div className="suna-chat-inline-status" aria-live="polite">
      <div className="suna-chat-inline-status-line">
        <span className={`suna-chat-inline-status-state ${stateTone(run.state)}`}>
          <Wrench size={13} aria-hidden="true" />
          {stateLabel(run.state, t)}
        </span>
        {run.toolCalls.length > 0 && <span>{t("agentToolCount", { count: run.toolCalls.length })}</span>}
        {run.taskProgress && <span>{run.taskProgress.kind} · {run.taskProgress.progress}%</span>}
        {canResume && (
          <button type="button" className="suna-action-secondary" onClick={onResume}>
            <Play size={13} aria-hidden="true" />{t("resumeAgentRun")}
          </button>
        )}
        {canRetry && (
          <button type="button" className="suna-action-secondary" onClick={onRetry}>
            <RotateCcw size={13} aria-hidden="true" />{t("retryAgentRun")}
          </button>
        )}
      </div>
      {run.toolCalls.length > 0 && (
        <div className="suna-chat-tool-trace" aria-label="Agent tools">
          {run.toolCalls.map((tool) => <span key={tool.toolCallId}>{tool.name} · {tool.progress}%</span>)}
        </div>
      )}
      <NativePermissionPanel permissions={run.permissions} onResolve={onResolvePermission} />
    </div>
  );
}

function NativeMessage({
  message,
  index,
  loading,
  onEdit,
  onResolvePermission,
  onFollowUp,
}: {
  message: Message;
  index: number;
  loading: boolean;
  onEdit: () => void;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
  onFollowUp: (question: string) => void;
}) {
  const response = parseWebResponse(message);
  const evidence = responseEvidence(message);
  if (!isAssistant(message)) {
    return (
      <div className="suna-chat-user-turn" data-agent-user-turn={index}>
        <div className="suna-chat-user-bubble">{message.content}</div>
        <div className="suna-chat-message-actions">
          <button type="button" aria-label="复制消息" title="复制消息" onClick={() => void copyText(message.content)}>
            <Copy size={15} aria-hidden="true" />
          </button>
          <button type="button" aria-label="编辑消息" title="编辑消息" onClick={onEdit}>
            <Pencil size={15} aria-hidden="true" />
          </button>
        </div>
      </div>
    );
  }

  const confirmations = response?.pending_confirmations ?? [];
  return (
    <article className="suna-chat-assistant-turn" aria-label="Suna">
      <div className="suna-chat-answer ai-markdown-body">
        <AIAnswerRenderer answer={message.content} literatureResults={[]} />
      </div>
      {response && (response.context_status.memory_count > 0 || response.context_status.skill_count > 0 || response.context_status.tool_count > 0) && (
        <div className="suna-chat-inline-status" aria-label={response.context_status ? "本轮上下文" : undefined}>
          <div className="suna-chat-inline-status-line">
            {response.context_status.memory_count > 0 && <span>{response.context_status.memory_count} 条记忆</span>}
            {response.context_status.skill_count > 0 && <span>{response.context_status.skill_count} 个技能</span>}
            {response.context_status.tool_count > 0 && <span>{response.context_status.tool_count} 个工具</span>}
          </div>
        </div>
      )}
      {evidence && <CitationPanel auditId={evidence.auditId} evidence={evidence.evidence} />}
      {response?.follow_up_questions.length ? (
        <div className="suna-chat-inline-status" aria-label="需要补充的信息">
          <strong>需要补充的信息</strong>
          <div className="suna-chat-permission-actions">
            {response.follow_up_questions.map((question) => (
              <button type="button" className="suna-action-secondary" key={question} onClick={() => onFollowUp(question)}>{question}</button>
            ))}
          </div>
        </div>
      ) : null}
      {response?.recommendations.length ? (
        <div className="suna-chat-inline-status" aria-label="推荐方案">
          <strong>推荐方案</strong>
          <div className="grid gap-3 xl:grid-cols-2">
            {response.recommendations.map((item, index) => <WebRecommendationCard key={`${item.title}-${index}`} item={item} />)}
          </div>
        </div>
      ) : null}
      {confirmations.length > 0 && (
        <div className="suna-chat-inline-status">
          <WebConfirmDialog
            confirmations={confirmations}
            onConfirm={(item: WebPendingConfirmation, approved) => onResolvePermission(item.action_id, approved ? "allow_once" : "deny")}
          />
        </div>
      )}
      {!loading && <WebFeedback messageId={message.id} />}
      <div className="suna-chat-message-actions">
        <button type="button" aria-label="复制回答" title="复制回答" onClick={() => void copyText(message.content)}>
          <Copy size={15} aria-hidden="true" />
        </button>
      </div>
    </article>
  );
}

function conversationTitle(conversation: Conversation) {
  return conversation.title.trim() || "新建对话";
}

export default function DesktopChatWorkspace({
  onOpenSection: _onOpenSection,
  ...controller
}: ChatControllerProps & { onOpenSection?: (section: SectionId) => void }) {
  const { t } = useLocale();
  const { preferences } = useAppearanceSettings();
  const [search, setSearch] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renamingTitle, setRenamingTitle] = useState("");
  const [menuId, setMenuId] = useState<string | null>(null);
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [knowledgeMenuOpen, setKnowledgeMenuOpen] = useState(false);
  const [sendShortcut, setSendShortcut] = useState("Ctrl+Enter");
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const imageInputRef = useRef<HTMLInputElement>(null);
  const messagesRef = useRef<HTMLDivElement>(null);
  const activeProfile = controller.chatProfiles.find((profile) => profile.id === controller.activeChatProfileId);
  const selectedModel = activeProfile?.model_id || activeProfile?.display_name || "本地模型";
  const conversations = controller.conversations.filter((conversation) => {
    if (conversation.archived) return false;
    return !search.trim() || conversation.title.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase());
  });

  useEffect(() => {
    let active = true;
    if (typeof desktop.getSetting !== "function") return () => { active = false; };
    void desktop.getSetting("ui.shortcuts").then((raw) => {
      if (!active || !raw) return;
      try {
        const value = JSON.parse(raw) as Record<string, unknown>;
        if (typeof value.send === "string" && value.send) setSendShortcut(value.send);
      } catch {
        // Keep the safe default when settings are unavailable or malformed.
      }
    }).catch(() => undefined);
    return () => { active = false; };
  }, []);

  const onComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    const parts = sendShortcut.toLowerCase().split("+");
    const key = parts[parts.length - 1] ?? "enter";
    const matches = event.key.toLowerCase() === key
      && event.ctrlKey === parts.includes("ctrl")
      && event.shiftKey === parts.includes("shift")
      && event.altKey === parts.includes("alt")
      && event.metaKey === parts.includes("meta");
    if (sendShortcut !== "未设置" && matches) {
      event.preventDefault();
      event.currentTarget.form?.requestSubmit();
    }
  };

  const encodeBytes = (bytes: Uint8Array) => {
    let binary = "";
    const chunkSize = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
    }
    return btoa(binary);
  };

  const onAttachmentFiles = (files: FileList | File[]) => {
    const accepted = Array.from(files).filter((file) => file.type.startsWith("image/") || /\.(pdf|docx?|xlsx?|csv|txt|md|markdown|json|html?)$/i.test(file.name));
    if (accepted.length === 0) return;
    void Promise.all(accepted.map((file) => new Promise<{ name: string; mime: string; data: string } | null>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => {
        const value = reader.result;
        const data = typeof value === "string"
          ? (file.type.startsWith("text/") || /\.(csv|txt|md|markdown|json|html?)$/i.test(file.name)
            ? encodeBytes(new TextEncoder().encode(value))
            : (value.includes(",") ? value.slice(value.indexOf(",") + 1) : value))
          : encodeBytes(new Uint8Array(value as ArrayBuffer));
        resolve(data ? { name: file.name || "attachment", mime: file.type || "application/octet-stream", data } : null);
      };
      reader.onerror = () => resolve(null);
      if (file.type.startsWith("text/") || /\.(csv|txt|md|markdown|json|html?)$/i.test(file.name)) reader.readAsText(file);
      else reader.readAsDataURL(file);
    }))).then((items) => {
      const next = items.filter((item): item is { name: string; mime: string; data: string } => item !== null);
      if (next.length > 0) controller.onAttachmentsChange([...controller.attachments, ...next]);
    });
  };

  const onFileInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    if (event.target.files) onAttachmentFiles(event.target.files);
    event.target.value = "";
  };

  const nativeMime = (name: string) => {
    const extension = name.toLowerCase().split(".").pop() || "";
    const known: Record<string, string> = {
      pdf: "application/pdf", doc: "application/msword", docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      xls: "application/vnd.ms-excel", xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      csv: "text/csv", txt: "text/plain", md: "text/markdown", markdown: "text/markdown", json: "application/json", html: "text/html", htm: "text/html",
    };
    return known[extension] || "application/octet-stream";
  };

  const pickNativeAttachments = async () => {
    try {
      const selected = await desktop.openFileDialog({
        multiple: true,
        directory: false,
        title: "添加 Agent 附件",
        filters: [{ name: "支持的文档附件", extensions: ["pdf", "doc", "docx", "xls", "xlsx", "csv", "txt", "md", "markdown", "json", "html", "htm"] }],
      });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      const next = paths.map((path) => ({
        name: path.split(/[\\/]/).pop() || "attachment",
        mime: nativeMime(path),
        data: "",
        path,
      }));
      if (next.length > 0) controller.onAttachmentsChange([...controller.attachments, ...next]);
    } catch {
      // Keep the composer usable when the native picker is unavailable.
    }
  };

  const beginRename = (conversation: Conversation) => {
    setMenuId(null);
    setRenamingId(conversation.id);
    setRenamingTitle(conversation.title);
  };

  const commitRename = () => {
    if (!renamingId) return;
    void controller.onRenameConversation(renamingId, renamingTitle);
    setRenamingId(null);
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (controller.pendingQuestion !== null) {
      void controller.onSteer(controller.draft);
      return;
    }
    void controller.onSubmit(event);
  };

  return (
    <section className={`suna-chat ${preferences.showAgentPanel ? "" : "has-agent-panel-hidden"}`} aria-label="本地智能体对话">
      <aside className="suna-chat-sidebar" aria-label={t("conversationList")}>
        <div className="suna-chat-sidebar-actions">
          <button type="button" className="suna-chat-sidebar-action is-primary" onClick={() => void controller.onNewConversation()}>
            <MessageSquarePlus size={17} aria-hidden="true" />
            <span>{t("newConversation")}</span>
          </button>
        </div>
        <label className="suna-chat-search">
          <Search size={15} aria-hidden="true" />
          <input
            type="search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("searchConversationsPlaceholder")}
            aria-label={t("searchConversations")}
          />
          {search && <button type="button" aria-label="清除搜索" title="清除搜索" onClick={() => setSearch("")}><X size={14} /></button>}
        </label>
        <p className="suna-chat-recent-heading">{t("chatRecent")}</p>
        <div className="suna-chat-session-list">
          {controller.loading ? (
            <div className="suna-chat-list-state"><LoaderCircle size={16} className="suna-spin" />{t("loading")}</div>
          ) : conversations.length === 0 ? (
            <div className="suna-chat-list-state">{search ? t("noMatchingConversations") : t("noLocalSessions")}</div>
          ) : conversations.map((conversation) => (
            <div className={`suna-chat-session-wrap ${conversation.id === controller.selectedId ? "is-active" : ""}`} key={conversation.id}>
              {renamingId === conversation.id ? (
                <input
                  className="suna-chat-session-rename"
                  value={renamingTitle}
                  autoFocus
                  onChange={(event) => setRenamingTitle(event.target.value)}
                  onBlur={commitRename}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") commitRename();
                    if (event.key === "Escape") setRenamingId(null);
                  }}
                  aria-label="重命名对话"
                />
              ) : (
                <button
                  type="button"
                  className={`suna-chat-session ${conversation.id === controller.selectedId ? "is-active" : ""}`}
                  onClick={() => controller.onSelectConversation(conversation.id)}
                >
                  <span>{conversationTitle(conversation)}</span>
                </button>
              )}
              {renamingId !== conversation.id && (
                <div className={`suna-chat-session-actions ${menuId === conversation.id ? "is-visible" : ""}`}>
                  <button type="button" className="suna-chat-session-action" aria-label="更多操作" title="更多操作" onClick={() => setMenuId(menuId === conversation.id ? null : conversation.id)}>
                    <MoreHorizontal size={15} aria-hidden="true" />
                  </button>
                </div>
              )}
              {menuId === conversation.id && (
                <div className="suna-chat-session-menu" role="menu">
                  <button type="button" role="menuitem" onClick={() => void controller.onToggleConversationPinned(conversation)}>
                    {conversation.pinned ? <PinOff size={14} /> : <Pin size={14} />}{conversation.pinned ? "取消置顶" : "置顶聊天"}
                  </button>
                  <button type="button" role="menuitem" onClick={() => beginRename(conversation)}><Pencil size={14} />{t("rename")}</button>
                  <button type="button" role="menuitem" onClick={() => void controller.onArchiveConversation(conversation.id)}><Archive size={14} />归档</button>
                  <button type="button" role="menuitem" className="is-danger" onClick={() => void controller.onDeleteConversation(conversation.id)}><Trash2 size={14} />{t("delete")}</button>
                </div>
              )}
            </div>
          ))}
        </div>
        <p className="suna-sidebar-footer">{t("chatSidebarFooter")}</p>
      </aside>

      <main className="suna-chat-main">
        <header className="suna-chat-header" style={{ justifyContent: "space-between" }}>
          <div>
            <p className="suna-eyebrow">{t("steelRuntime")}</p>
            <h2>{controller.selectedConversation?.title ?? t("chatTitle")}</h2>
          </div>
          <div className="suna-chat-header-actions">
            <span className="suna-chat-runtime"><span className="suna-state-dot" />{t("localAgent")}</span>
            {controller.selectedConversation && (
              <>
                <button type="button" className="suna-icon-button" aria-label={t("chatExportMarkdown")} title={t("chatExportMarkdown")} onClick={() => controller.onExportConversation("markdown")}><Download size={16} /></button>
                <button type="button" className="suna-icon-button" aria-label={t("chatExportJson")} title={t("chatExportJson")} onClick={() => controller.onExportConversation("json")}><FileJson size={16} /></button>
              </>
            )}
          </div>
        </header>

        {(controller.error || controller.notice) && (
          <div className="suna-chat-main-alerts">
            {controller.error && <div className="suna-knowledge-alert" role="alert">{controller.error}</div>}
            {controller.notice && <div className="suna-knowledge-notice" role="status">{controller.notice}</div>}
          </div>
        )}

        <div ref={messagesRef} className="suna-chat-messages" aria-live="polite">
          {controller.loadingMessages ? (
            <div className="suna-chat-empty"><LoaderCircle size={20} className="suna-spin" /><span>{t("loading")}</span></div>
          ) : controller.messages.length === 0 && controller.pendingQuestion === null ? (
            <div className="suna-chat-empty suna-chat-empty-large suna-new-welcome">
              <div className="suna-welcome-brand"><span className="suna-welcome-mark"><SunaLogo size={58} title="Suna" /></span><div><strong>Suna</strong><small>钢铁材料智能体平台</small></div></div>
              <h1>你好，我是 <em>Suna</em></h1>
              <span className="suna-welcome-compat-copy">从一个具体问题开始</span>
              <span className="suna-welcome-compat-copy">例如：比较 Q345B 与 Q355B 的屈服强度要求，并指出适用标准。</span>
              <p>我可以帮助你进行钢铁材料的专业分析与问答，覆盖材料、工艺、性能、文献和数据等多个领域。</p>
              <div className="suna-welcome-cards">
                {[{ icon: "◈", title: "智能问答", text: "多 Agent 协同，精准解答复杂问题" }, { icon: "▣", title: "知识中心", text: "构建专属钢铁知识库" }, { icon: "▤", title: "文献研究", text: "文献检索、总结、对比" }, { icon: "▥", title: "数据实验室", text: "数据处理、分析、可视化" }, { icon: "△", title: "性能预测", text: "多模型预测材料性能" }, { icon: "✥", title: "工艺优化", text: "多目标优化算法" }, { icon: "♜", title: "实验助手", text: "智能设计实验方案" }, { icon: "✦", title: "Agent 管理", text: "多智能体协同与配置" }].map((item) => <button type="button" className="suna-welcome-card" key={item.title} onClick={() => controller.onDraftChange(`${item.title}：`)}><span>{item.icon}</span><strong>{item.title}</strong><small>{item.text}</small><b>→</b></button>)}
              </div>
            </div>
          ) : (
            <>
              {controller.messages.map((message, index) => (
                <NativeMessage
                  key={message.id}
                  message={message}
                  index={index}
                  loading={controller.pendingQuestion !== null}
                  onEdit={() => {
                    controller.onDraftChange(message.content);
                    inputRef.current?.focus();
                  }}
                  onFollowUp={(question) => {
                    controller.onDraftChange(question);
                    inputRef.current?.focus();
                  }}
                  onResolvePermission={controller.onResolvePermission}
                />
              ))}
              {controller.pendingQuestion && (
                <>
                  <div className="suna-chat-user-turn" data-agent-user-turn="pending">
                    <div className="suna-chat-user-bubble">{controller.pendingQuestion}</div>
                  </div>
                  <article className="suna-chat-assistant-turn is-streaming" aria-label="Suna">
                    <div className="suna-chat-answer ai-markdown-body">
                      <AIAnswerRenderer answer={controller.agentRun?.assistantText || t("contextPreparing")} literatureResults={[]} />
                      {controller.agentRun?.assistantText && <span className="ai-typing-cursor" aria-hidden="true" />}
                    </div>
                    {controller.agentRun?.evidencePackId && controller.streamingCitations.length > 0 && <CitationPanel auditId={controller.agentRun.evidencePackId} evidence={controller.streamingCitations} />}
                  </article>
                </>
              )}
          <NativeRunStatus
            run={controller.agentRun}
            recovery={controller.recovery}
            onResolvePermission={controller.onResolvePermission}
            onRetry={controller.onRetry}
            onResume={controller.onResume}
          />
            </>
          )}
          <WebTurnNavigator
            messages={controller.messages.map(toWebMessage)}
            scrollContainerRef={messagesRef}
          />
        </div>

        <form className="suna-chat-composer" data-testid="desktop-agent-composer" onSubmit={submit}>
          {controller.attachments.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-2" aria-label="已添加附件">
              {controller.attachments.map((attachment, index) => (
                <div className="group/attachment relative h-16 w-16 overflow-hidden rounded-lg border border-[var(--suna-line)] bg-[var(--suna-bg-soft)]" key={`${attachment.name}-${index}`}>
                  {attachment.mime.startsWith("image/") ? <img src={`data:${attachment.mime};base64,${attachment.data}`} alt={attachment.name} className="h-full w-full object-cover" /> : <div className="flex h-full w-full flex-col items-center justify-center gap-1 p-1 text-[var(--suna-text-muted)]"><FileText size={22} /><span className="max-w-full truncate text-[10px]">{attachment.name}</span></div>}
                  <button type="button" className="absolute right-0.5 top-0.5 flex h-5 w-5 items-center justify-center rounded-full bg-black/55 text-white" aria-label={`移除图片 ${attachment.name}`} title={`移除图片 ${attachment.name}`} onClick={() => controller.onAttachmentsChange(controller.attachments.filter((_, itemIndex) => itemIndex !== index))}><X size={12} /></button>
                </div>
              ))}
            </div>
          )}
          <textarea
            ref={inputRef}
            value={controller.draft}
            onChange={(event) => controller.onDraftChange(event.target.value)}
            onKeyDown={onComposerKeyDown}
            onPaste={(event) => {
              const files = event.clipboardData?.files;
              if (files && files.length > 0) {
                event.preventDefault();
                onAttachmentFiles(files);
              }
            }}
            aria-label={t("inputMessage")}
            placeholder={t("askPlaceholder")}
            rows={3}
            disabled={false}
          />
          <div className="suna-chat-composer-footer">
            <div className="suna-chat-composer-tools">
              <button type="button" className={`suna-chat-composer-tool ${controller.smartSearchEnabled || controller.autoKnowledgeSearchEnabled ? "is-active" : ""}`} aria-label="智能搜索" aria-pressed={controller.smartSearchEnabled || controller.autoKnowledgeSearchEnabled} title="使用本地知识库检索" onClick={controller.onToggleSmartSearch} disabled={controller.pendingQuestion !== null}><Globe size={15} /><span>智能搜索</span></button>
              <button type="button" className="suna-chat-composer-tool" aria-label="添加图片" title="添加图片" onClick={() => imageInputRef.current?.click()} disabled={controller.pendingQuestion !== null}><MessageSquarePlus size={15} /><span>图片</span></button>
              <button type="button" className="suna-chat-composer-tool" aria-label="添加附件" title="添加 PDF、Office、Markdown 或文本附件" onClick={() => void pickNativeAttachments()} disabled={controller.pendingQuestion !== null}><FileText size={15} /><span>附件</span></button>
              <input ref={imageInputRef} type="file" accept="image/*" multiple hidden onChange={onFileInputChange} />
              <input ref={fileInputRef} type="file" accept="image/*,.pdf,.doc,.docx,.xls,.xlsx,.csv,.txt,.md,.markdown,.json,.html,.htm" multiple hidden onChange={onFileInputChange} />
            </div>
            <div className="suna-chat-composer-right">
              {controller.pendingQuestion !== null && (
                <>
                  <button
                    type="button"
                    className="suna-chat-composer-tool"
                    aria-label="追加消息"
                    title="在本轮结束后追加消息"
                    disabled={!controller.draft.trim()}
                    onClick={() => void controller.onFollowUp(controller.draft)}
                  >
                    <CornerDownLeft size={15} aria-hidden="true" /><span>追加</span>
                  </button>
                  <button
                    type="button"
                    className="suna-chat-composer-tool"
                    aria-label="转向当前运行"
                    title="转向当前运行"
                    disabled={!controller.draft.trim()}
                    onClick={() => void controller.onSteer(controller.draft)}
                  >
                    <ArrowUp size={15} aria-hidden="true" /><span>转向</span>
                  </button>
                </>
              )}
              <div className="suna-chat-model-picker">
                {modelMenuOpen && (
                  <div className="suna-chat-model-menu" role="menu">
                    {controller.chatProfiles.length === 0 ? <span className="suna-chat-model-empty">请先在设置中配置聊天模型</span> : controller.chatProfiles.map((profile) => (
                      <button type="button" role="menuitem" className={profile.id === controller.activeChatProfileId ? "is-active" : ""} key={profile.id} onClick={() => { setModelMenuOpen(false); controller.onSelectChatProfile(profile.id); }}>
                        <span>{profile.model_id || profile.display_name}</span>
                        {profile.id === controller.activeChatProfileId && <Check size={14} />}
                      </button>
                    ))}
                  </div>
                )}
                <button type="button" className="suna-chat-model-button" aria-label="切换当前对话模型" title="切换当前对话模型" aria-expanded={modelMenuOpen} onClick={() => setModelMenuOpen((open) => !open)} disabled={controller.pendingQuestion !== null}>
                  <span>{selectedModel}</span><ChevronDown size={14} className={modelMenuOpen ? "is-open" : undefined} />
                </button>
              </div>
              <div className="suna-chat-model-picker">
                {agentMenuOpen && (
                  <div className="suna-chat-model-menu" role="menu">
                    <button type="button" role="menuitem" className={controller.activeAgentId === null ? "is-active" : ""} onClick={() => { setAgentMenuOpen(false); controller.onSelectAgent(null); }}>
                      <span>自动选择 Agent</span>{controller.activeAgentId === null && <Check size={14} />}
                    </button>
                    {controller.agentProfiles.map((profile) => (
                      <button type="button" role="menuitem" className={profile.id === controller.activeAgentId ? "is-active" : ""} key={profile.id} onClick={() => { setAgentMenuOpen(false); controller.onSelectAgent(profile.id); }}>
                        <span>{profile.name}</span>{profile.id === controller.activeAgentId && <Check size={14} />}
                      </button>
                    ))}
                  </div>
                )}
                <button type="button" className="suna-chat-model-button" aria-label="选择 Agent" title="选择本轮使用的 Agent" aria-expanded={agentMenuOpen} onClick={() => { setAgentMenuOpen((open) => !open); setModelMenuOpen(false); }} disabled={controller.pendingQuestion !== null}>
                  <span>{controller.activeAgentId ? controller.agentProfiles.find((profile) => profile.id === controller.activeAgentId)?.name ?? controller.activeAgentId : "自动 Agent"}</span><ChevronDown size={14} className={agentMenuOpen ? "is-open" : undefined} />
                </button>
              </div>
              <div className="suna-chat-model-picker">
                {knowledgeMenuOpen && (
                  <div className="suna-chat-model-menu" role="menu" aria-label="选择知识库">
                    {controller.knowledgeBases.length === 0 ? <span className="suna-chat-model-empty">暂无可用知识库</span> : controller.knowledgeBases.map((base) => {
                      const checked = controller.selectedKnowledgeBaseIds.includes(base.id);
                      return <button type="button" role="menuitemcheckbox" aria-checked={checked} className={checked ? "is-active" : ""} key={base.id} onClick={() => controller.onSelectKnowledgeBases(checked ? controller.selectedKnowledgeBaseIds.filter((id) => id !== base.id) : [...controller.selectedKnowledgeBaseIds, base.id])}><span>{base.name}</span>{checked && <Check size={14} />}</button>;
                    })}
                  </div>
                )}
                <button type="button" className="suna-chat-model-button" aria-label="选择知识库" title="选择本轮检索的知识库" aria-expanded={knowledgeMenuOpen} onClick={() => { setKnowledgeMenuOpen((open) => !open); setModelMenuOpen(false); setAgentMenuOpen(false); }} disabled={controller.pendingQuestion !== null}>
                  <span>{controller.selectedKnowledgeBaseIds.length === 0 ? "无知识库" : controller.selectedKnowledgeBaseIds.length === controller.knowledgeBases.length ? "全部知识库" : `知识库 ${controller.selectedKnowledgeBaseIds.length}`}</span><ChevronDown size={14} className={knowledgeMenuOpen ? "is-open" : undefined} />
                </button>
              </div>
              <button type={controller.pendingQuestion ? "button" : "submit"} className={`suna-chat-send-button ${controller.pendingQuestion ? "is-stop" : ""}`} aria-label={controller.pendingQuestion ? t("stopGenerating") : t("send")} title={controller.pendingQuestion ? t("stopGenerating") : t("send")} disabled={!controller.pendingQuestion && !controller.draft.trim() && controller.attachments.length === 0} onClick={controller.pendingQuestion ? controller.onCancel : undefined}>
                {controller.pendingQuestion ? <Square size={15} fill="currentColor" /> : <ArrowUp size={20} strokeWidth={2.6} />}
              </button>
            </div>
          </div>
        </form>
      </main>

      {preferences.showAgentPanel && <AgentRunInspector
          run={controller.agentRun}
          recovery={controller.recovery}
          showToolDetails={preferences.showToolDetails}
          onResolvePermission={controller.onResolvePermission}
          onRetry={controller.onRetry}
          onResume={controller.onResume}
        />}
    </section>
  );
}
