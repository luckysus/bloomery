import { useRef, useState, type ChangeEvent, type FormEvent, type KeyboardEvent } from "react";
import {
  Archive,
  ArrowUp,
  Check,
  ChevronDown,
  Copy,
  Download,
  FileJson,
  Globe,
  LoaderCircle,
  MessageSquarePlus,
  MoreHorizontal,
  Pencil,
  Pin,
  PinOff,
  Search,
  ShieldAlert,
  Sparkles,
  Square,
  Trash2,
  Wrench,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import type { Conversation, Message } from "../../bridge/desktop";
import type { PermissionDecision } from "../../bridge/generated/protocol";
import AIAnswerRenderer from "../../components/answer/AnswerRenderer";
import CitationPanel from "./CitationPanel";
import type { AgentPermissionView, AgentRunView } from "./agentEvents";
import type { ChatControllerProps } from "./chatController";
import { parseWebResponse, toWebMessage, type WebPendingConfirmation } from "./web/webTypes";
import WebConfirmDialog from "./web/WebConfirmDialog";
import WebFeedback from "./web/WebFeedback";
import WebRecommendationCard from "./web/WebRecommendationCard";
import WebTurnNavigator from "./web/WebTurnNavigator";

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
    <div className="bloomery-chat-permission-list" role="alert">
      {pending.map((permission) => (
        <div className="bloomery-chat-permission" key={permission.permissionId}>
          <div className="bloomery-chat-permission-heading">
            <ShieldAlert size={16} aria-hidden="true" />
            <div>
              <strong>{t("permissionRequired")}</strong>
              <span>{permission.summary}</span>
            </div>
          </div>
          <p>{permission.reason}</p>
          <div className="bloomery-chat-permission-actions">
            {actions.map(({ decision, label }) => (
              <button
                type="button"
                className={decision === "deny" ? "bloomery-action-secondary" : "bloomery-action-primary"}
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
  onResolvePermission,
}: {
  run: AgentRunView | null;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const { t } = useLocale();
  if (!run) return null;
  const hasPendingPermission = run.permissions.some((permission) => permission.decision === null);
  const settled = ["completed", "failed", "cancelled", "interrupted"].includes(run.state);
  if (settled && !hasPendingPermission && run.toolCalls.length === 0) return null;

  return (
    <div className="bloomery-chat-inline-status" aria-live="polite">
      <div className="bloomery-chat-inline-status-line">
        <span className={`bloomery-chat-inline-status-state ${stateTone(run.state)}`}>
          <Wrench size={13} aria-hidden="true" />
          {stateLabel(run.state, t)}
        </span>
        {run.toolCalls.length > 0 && <span>{t("agentToolCount", { count: run.toolCalls.length })}</span>}
        {run.taskProgress && <span>{run.taskProgress.kind} · {run.taskProgress.progress}%</span>}
      </div>
      {run.toolCalls.length > 0 && (
        <div className="bloomery-chat-tool-trace" aria-label="Agent tools">
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
      <div className="bloomery-chat-user-turn" data-agent-user-turn={index}>
        <div className="bloomery-chat-user-bubble">{message.content}</div>
        <div className="bloomery-chat-message-actions">
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
    <article className="bloomery-chat-assistant-turn" aria-label="Bloomery">
      <div className="bloomery-chat-answer ai-markdown-body">
        <AIAnswerRenderer answer={message.content} literatureResults={[]} />
      </div>
      {response && (response.context_status.memory_count > 0 || response.context_status.skill_count > 0 || response.context_status.tool_count > 0) && (
        <div className="bloomery-chat-inline-status" aria-label={response.context_status ? "本轮上下文" : undefined}>
          <div className="bloomery-chat-inline-status-line">
            {response.context_status.memory_count > 0 && <span>{response.context_status.memory_count} 条记忆</span>}
            {response.context_status.skill_count > 0 && <span>{response.context_status.skill_count} 个技能</span>}
            {response.context_status.tool_count > 0 && <span>{response.context_status.tool_count} 个工具</span>}
          </div>
        </div>
      )}
      {evidence && <CitationPanel auditId={evidence.auditId} evidence={evidence.evidence} />}
      {response?.follow_up_questions.length ? (
        <div className="bloomery-chat-inline-status" aria-label="需要补充的信息">
          <strong>需要补充的信息</strong>
          <div className="bloomery-chat-permission-actions">
            {response.follow_up_questions.map((question) => (
              <button type="button" className="bloomery-action-secondary" key={question} onClick={() => onFollowUp(question)}>{question}</button>
            ))}
          </div>
        </div>
      ) : null}
      {response?.recommendations.length ? (
        <div className="bloomery-chat-inline-status" aria-label="推荐方案">
          <strong>推荐方案</strong>
          <div className="grid gap-3 xl:grid-cols-2">
            {response.recommendations.map((item, index) => <WebRecommendationCard key={`${item.title}-${index}`} item={item} />)}
          </div>
        </div>
      ) : null}
      {confirmations.length > 0 && (
        <div className="bloomery-chat-inline-status">
          <WebConfirmDialog
            confirmations={confirmations}
            onConfirm={(item: WebPendingConfirmation, approved) => onResolvePermission(item.action_id, approved ? "allow_once" : "deny")}
          />
        </div>
      )}
      {!loading && <WebFeedback messageId={message.id} />}
      <div className="bloomery-chat-message-actions">
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
}: ChatControllerProps & { onOpenSection?: (section: "workbench" | "chat" | "knowledge" | "databases" | "analysis" | "extensions" | "settings" | "diagnostics") => void }) {
  const { t } = useLocale();
  const [search, setSearch] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renamingTitle, setRenamingTitle] = useState("");
  const [menuId, setMenuId] = useState<string | null>(null);
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const messagesRef = useRef<HTMLDivElement>(null);
  const activeProfile = controller.chatProfiles.find((profile) => profile.id === controller.activeChatProfileId);
  const selectedModel = activeProfile?.model_id || activeProfile?.display_name || "本地模型";
  const conversations = controller.conversations.filter((conversation) => {
    if (conversation.archived) return false;
    return !search.trim() || conversation.title.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase());
  });

  const onComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      event.currentTarget.form?.requestSubmit();
    }
  };

  const onImageFiles = (files: FileList | File[]) => {
    const images = Array.from(files).filter((file) => file.type.startsWith("image/"));
    if (images.length === 0) return;
    void Promise.all(images.map((file) => new Promise<{ name: string; mime: string; data: string } | null>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = String(reader.result || "");
        const data = result.includes(",") ? result.slice(result.indexOf(",") + 1) : result;
        resolve(data ? { name: file.name || "image", mime: file.type || "image/png", data } : null);
      };
      reader.onerror = () => resolve(null);
      reader.readAsDataURL(file);
    }))).then((items) => {
      const next = items.filter((item): item is { name: string; mime: string; data: string } => item !== null);
      if (next.length > 0) controller.onAttachmentsChange([...controller.attachments, ...next]);
    });
  };

  const onFileInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    if (event.target.files) onImageFiles(event.target.files);
    event.target.value = "";
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
    void controller.onSubmit(event);
  };

  return (
    <section className="bloomery-chat" aria-label="本地智能体对话">
      <aside className="bloomery-chat-sidebar" aria-label={t("conversationList")}>
        <div className="bloomery-chat-sidebar-actions">
          <button type="button" className="bloomery-chat-sidebar-action is-primary" onClick={() => void controller.onNewConversation()}>
            <MessageSquarePlus size={17} aria-hidden="true" />
            <span>{t("newConversation")}</span>
          </button>
        </div>
        <label className="bloomery-chat-search">
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
        <p className="bloomery-chat-recent-heading">{t("chatRecent")}</p>
        <div className="bloomery-chat-session-list">
          {controller.loading ? (
            <div className="bloomery-chat-list-state"><LoaderCircle size={16} className="bloomery-spin" />{t("loading")}</div>
          ) : conversations.length === 0 ? (
            <div className="bloomery-chat-list-state">{search ? t("noMatchingConversations") : t("noLocalSessions")}</div>
          ) : conversations.map((conversation) => (
            <div className={`bloomery-chat-session-wrap ${conversation.id === controller.selectedId ? "is-active" : ""}`} key={conversation.id}>
              {renamingId === conversation.id ? (
                <input
                  className="bloomery-chat-session-rename"
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
                  className={`bloomery-chat-session ${conversation.id === controller.selectedId ? "is-active" : ""}`}
                  onClick={() => controller.onSelectConversation(conversation.id)}
                >
                  <span>{conversationTitle(conversation)}</span>
                </button>
              )}
              {renamingId !== conversation.id && (
                <div className={`bloomery-chat-session-actions ${menuId === conversation.id ? "is-visible" : ""}`}>
                  <button type="button" className="bloomery-chat-session-action" aria-label="更多操作" title="更多操作" onClick={() => setMenuId(menuId === conversation.id ? null : conversation.id)}>
                    <MoreHorizontal size={15} aria-hidden="true" />
                  </button>
                </div>
              )}
              {menuId === conversation.id && (
                <div className="bloomery-chat-session-menu" role="menu">
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
        <p className="bloomery-sidebar-footer">{t("chatSidebarFooter")}</p>
      </aside>

      <main className="bloomery-chat-main">
        <header className="bloomery-chat-header" style={{ justifyContent: "space-between" }}>
          <div>
            <p className="bloomery-eyebrow">{t("steelRuntime")}</p>
            <h2>{controller.selectedConversation?.title ?? t("chatTitle")}</h2>
          </div>
          <div className="bloomery-chat-header-actions">
            <span className="bloomery-chat-runtime"><span className="bloomery-state-dot" />{t("localAgent")}</span>
            {controller.selectedConversation && (
              <>
                <button type="button" className="bloomery-icon-button" aria-label={t("chatExportMarkdown")} title={t("chatExportMarkdown")} onClick={() => controller.onExportConversation("markdown")}><Download size={16} /></button>
                <button type="button" className="bloomery-icon-button" aria-label={t("chatExportJson")} title={t("chatExportJson")} onClick={() => controller.onExportConversation("json")}><FileJson size={16} /></button>
              </>
            )}
          </div>
        </header>

        {(controller.error || controller.notice) && (
          <div className="bloomery-chat-main-alerts">
            {controller.error && <div className="bloomery-knowledge-alert" role="alert">{controller.error}</div>}
            {controller.notice && <div className="bloomery-knowledge-notice" role="status">{controller.notice}</div>}
          </div>
        )}

        <div ref={messagesRef} className="bloomery-chat-messages" aria-live="polite">
          {controller.loadingMessages ? (
            <div className="bloomery-chat-empty"><LoaderCircle size={20} className="bloomery-spin" /><span>{t("loading")}</span></div>
          ) : controller.messages.length === 0 && controller.pendingQuestion === null ? (
            <div className="bloomery-chat-empty bloomery-chat-empty-large">
              <span className="bloomery-chat-empty-icon"><Sparkles size={22} /></span>
              <strong>{t("startSpecificQuestion")}</strong>
              <span>{t("exampleQuestion")}</span>
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
                  <div className="bloomery-chat-user-turn" data-agent-user-turn="pending">
                    <div className="bloomery-chat-user-bubble">{controller.pendingQuestion}</div>
                  </div>
                  <article className="bloomery-chat-assistant-turn is-streaming" aria-label="Bloomery">
                    <div className="bloomery-chat-answer ai-markdown-body">
                      <AIAnswerRenderer answer={controller.agentRun?.assistantText || t("contextPreparing")} literatureResults={[]} />
                      {controller.agentRun?.assistantText && <span className="ai-typing-cursor" aria-hidden="true" />}
                    </div>
                  </article>
                </>
              )}
              <NativeRunStatus run={controller.agentRun} onResolvePermission={controller.onResolvePermission} />
            </>
          )}
          <WebTurnNavigator
            messages={controller.messages.map(toWebMessage)}
            scrollContainerRef={messagesRef}
          />
        </div>

        <form className="bloomery-chat-composer" data-testid="desktop-agent-composer" onSubmit={submit}>
          {controller.attachments.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-2" aria-label="已添加图片">
              {controller.attachments.map((attachment, index) => (
                <div className="group/attachment relative h-16 w-16 overflow-hidden rounded-lg border border-[var(--bloomery-line)] bg-[var(--bloomery-bg-soft)]" key={`${attachment.name}-${index}`}>
                  <img src={`data:${attachment.mime};base64,${attachment.data}`} alt={attachment.name} className="h-full w-full object-cover" />
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
              if (files && Array.from(files).some((file) => file.type.startsWith("image/"))) {
                event.preventDefault();
                onImageFiles(files);
              }
            }}
            aria-label={t("inputMessage")}
            placeholder={t("askPlaceholder")}
            rows={3}
            disabled={controller.pendingQuestion !== null}
          />
          <div className="bloomery-chat-composer-footer">
            <div className="bloomery-chat-composer-tools">
              <button type="button" className={`bloomery-chat-composer-tool ${controller.smartSearchEnabled ? "is-active" : ""}`} aria-label="智能搜索" aria-pressed={controller.smartSearchEnabled} title="使用本地知识库检索" onClick={controller.onToggleSmartSearch} disabled={controller.pendingQuestion !== null}><Globe size={15} /><span>智能搜索</span></button>
              <button type="button" className="bloomery-chat-composer-tool" aria-label="添加图片" title="添加图片" onClick={() => fileInputRef.current?.click()} disabled={controller.pendingQuestion !== null}><MessageSquarePlus size={15} /><span>图片</span></button>
              <input ref={fileInputRef} type="file" accept="image/*" multiple hidden onChange={onFileInputChange} />
            </div>
            <div className="bloomery-chat-composer-right">
              <div className="bloomery-chat-model-picker">
                {modelMenuOpen && (
                  <div className="bloomery-chat-model-menu" role="menu">
                    {controller.chatProfiles.length === 0 ? <span className="bloomery-chat-model-empty">请先在设置中配置聊天模型</span> : controller.chatProfiles.map((profile) => (
                      <button type="button" role="menuitem" className={profile.id === controller.activeChatProfileId ? "is-active" : ""} key={profile.id} onClick={() => { setModelMenuOpen(false); controller.onSelectChatProfile(profile.id); }}>
                        <span>{profile.model_id || profile.display_name}</span>
                        {profile.id === controller.activeChatProfileId && <Check size={14} />}
                      </button>
                    ))}
                  </div>
                )}
                <button type="button" className="bloomery-chat-model-button" aria-label="切换当前对话模型" title="切换当前对话模型" aria-expanded={modelMenuOpen} onClick={() => setModelMenuOpen((open) => !open)} disabled={controller.pendingQuestion !== null}>
                  <span>{selectedModel}</span><ChevronDown size={14} className={modelMenuOpen ? "is-open" : undefined} />
                </button>
              </div>
              <button type={controller.pendingQuestion ? "button" : "submit"} className={`bloomery-chat-send-button ${controller.pendingQuestion ? "is-stop" : ""}`} aria-label={controller.pendingQuestion ? t("stopGenerating") : t("send")} title={controller.pendingQuestion ? t("stopGenerating") : t("send")} disabled={!controller.pendingQuestion && !controller.draft.trim() && controller.attachments.length === 0} onClick={controller.pendingQuestion ? controller.onCancel : undefined}>
                {controller.pendingQuestion ? <Square size={15} fill="currentColor" /> : <ArrowUp size={20} strokeWidth={2.6} />}
              </button>
            </div>
          </div>
        </form>
      </main>
    </section>
  );
}
