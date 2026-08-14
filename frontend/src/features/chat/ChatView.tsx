import type { FormEvent } from "react";
import {
  Activity,
  BookOpen,
  Bot,
  Check,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Cpu,
  Download,
  FileJson,
  LoaderCircle,
  MessageSquarePlus,
  PanelLeft,
  ShieldAlert,
  Send,
  Sparkles,
  Square,
  Wrench,
  X,
} from "lucide-react";
import AIAnswerRenderer from "../../components/answer/AnswerRenderer";
import { useLocale } from "../../i18n/locale";
import { type Conversation, type ConversationExportFormat, type EvidenceItem, type Message } from "../../bridge/desktop";
import type { AgentRunState, PermissionDecision } from "../../bridge/generated/protocol";
import CitationPanel from "./CitationPanel";
import type { AgentPermissionView, AgentRunView } from "./agentEvents";

interface ChatViewProps {
  conversations: Conversation[];
  selectedId: string | null;
  selectedConversation: Conversation | null;
  messages: Message[];
  loading: boolean;
  loadingMessages: boolean;
  draft: string;
  pendingQuestion: string | null;
  agentRun: AgentRunView | null;
  error: string | null;
  notice: string | null;
  onNewConversation: () => void;
  onSelectConversation: (id: string) => void;
  onDraftChange: (value: string) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCancel: () => void;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
  onExportConversation: (format: ConversationExportFormat) => void;
}

function isAssistant(message: Message) {
  return message.role === "agent" || message.role === "assistant";
}

function agentStateLabel(
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

function messageEvidence(message: Message) {
  if (!isAssistant(message) || !message.response_json) return null;
  try {
    const response = JSON.parse(message.response_json) as { evidence_pack_id?: unknown; evidence?: unknown };
    if (typeof response.evidence_pack_id !== "string" || !Array.isArray(response.evidence)) return null;
    return { evidencePackId: response.evidence_pack_id, evidence: response.evidence as EvidenceItem[] };
  } catch {
    return null;
  }
}

function toolTone(status: string) {
  if (status === "succeeded" || status === "completed" || status.startsWith("allow_")) return "is-good";
  if (status === "failed" || status === "cancelled" || status === "interrupted" || status === "deny") return "is-danger";
  return "is-pending";
}

function PermissionPanel({
  permissions,
  onResolvePermission,
}: {
  permissions: AgentPermissionView[];
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const { t } = useLocale();
  const pendingPermissions = permissions.filter((permission) => permission.decision === null);
  if (pendingPermissions.length === 0) return null;

  const actions: Array<{ decision: PermissionDecision; label: string; ariaLabel: string; icon: typeof Check }> = [
    { decision: "allow_once", label: t("allowOnce"), ariaLabel: "Allow once", icon: Check },
    { decision: "allow_session", label: t("allowSession"), ariaLabel: "Allow for session", icon: Check },
    { decision: "allow_always", label: t("allowAlways"), ariaLabel: "Always allow", icon: Check },
    { decision: "deny", label: t("deny"), ariaLabel: "Deny", icon: X },
  ];

  return (
    <div className="bloomery-chat-permission-list" aria-label={t("permissionRequired")}>
      {pendingPermissions.map((permission) => (
        <section className="bloomery-chat-permission" key={permission.permissionId} role="alert">
          <div className="bloomery-chat-permission-heading">
            <ShieldAlert size={18} aria-hidden="true" />
            <div>
              <strong>{permission.summary}</strong>
              <span>{t("permissionRequired")}</span>
            </div>
          </div>
          <p>{permission.reason}</p>
          <div className="bloomery-chat-permission-actions">
            {actions.map(({ decision, label, ariaLabel, icon: Icon }) => (
              <button
                className={decision === "deny" ? "bloomery-action-secondary is-danger" : "bloomery-action-secondary"}
                aria-label={`${ariaLabel} / ${label}`}
                key={decision}
                type="button"
                onClick={() => onResolvePermission(permission.permissionId, decision)}
              >
                <Icon size={14} aria-hidden="true" />
                {label}
              </button>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function RunInspector({
  agentRun,
  onResolvePermission,
}: {
  agentRun: AgentRunView | null;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const { t } = useLocale();
  const pendingPermissions = agentRun?.permissions.filter((permission) => permission.decision === null) ?? [];

  return (
    <aside className="bloomery-chat-inspector" aria-label={t("runtimeStatus")} data-testid="chat-inspector">
      <div className="bloomery-chat-inspector-header">
        <div>
          <p className="bloomery-eyebrow">{t("steelRuntime")}</p>
          <h3>{t("runtimeStatus")}</h3>
        </div>
        <span className={`bloomery-chat-inspector-state ${agentRun ? toolTone(agentRun.state) : "is-neutral"}`}>
          <Activity size={14} aria-hidden="true" />
          {agentRun ? agentStateLabel(agentRun.state, t) : t("localAgent")}
        </span>
      </div>

      {!agentRun ? (
        <div className="bloomery-chat-inspector-empty">
          <span className="bloomery-chat-inspector-empty-icon" aria-hidden="true"><Cpu size={20} /></span>
          <strong>{t("localAgent")}</strong>
          <p>{t("localRuntime")}</p>
        </div>
      ) : (
        <>
          <div className="bloomery-chat-inspector-metrics" aria-label={t("runtimeStatus")}>
            <div>
              <span><Wrench size={14} aria-hidden="true" />{t("agentToolCount", { count: agentRun.toolCalls.length })}</span>
              <strong>{agentRun.toolCalls.length}</strong>
            </div>
            <div>
              <span><BookOpen size={14} aria-hidden="true" />{t("citationSection")}</span>
              <strong>{agentRun.citationNumbers.length}</strong>
            </div>
          </div>

          {agentRun.toolCalls.length > 0 && (
            <section className="bloomery-chat-inspector-section" aria-labelledby="chat-tools-heading">
              <div className="bloomery-chat-inspector-section-heading">
                <h4 id="chat-tools-heading"><Wrench size={15} aria-hidden="true" />{t("agentToolCount", { count: agentRun.toolCalls.length })}</h4>
              </div>
              <div className="bloomery-chat-inspector-tool-list">
                {agentRun.toolCalls.map((tool) => (
                  <div className="bloomery-chat-inspector-tool" key={tool.toolCallId}>
                    <span className={`bloomery-chat-inspector-tool-icon ${toolTone(tool.status)}`} aria-hidden="true">
                      {tool.status === "succeeded" ? <CheckCircle2 size={14} /> : tool.status === "failed" ? <CircleAlert size={14} /> : <Clock3 size={14} />}
                    </span>
                    <div>
                      <strong>{tool.name}</strong>
                      <span>{t("agentToolProgress", { name: tool.status, progress: tool.progress })}</span>
                      {tool.message && <small>{tool.message}</small>}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}

          {pendingPermissions.length > 0 && (
            <section className="bloomery-chat-inspector-section bloomery-chat-inspector-permissions" aria-labelledby="chat-permissions-heading">
              <div className="bloomery-chat-inspector-section-heading">
                <h4 id="chat-permissions-heading"><ShieldAlert size={15} aria-hidden="true" />{t("permissionRequired")}</h4>
              </div>
              <PermissionPanel permissions={agentRun.permissions} onResolvePermission={onResolvePermission} />
            </section>
          )}

          {agentRun.taskProgress && (
            <section className="bloomery-chat-inspector-section" aria-labelledby="chat-task-heading">
              <div className="bloomery-chat-inspector-section-heading">
                <h4 id="chat-task-heading"><Clock3 size={15} aria-hidden="true" />{t("backgroundTasks")}</h4>
                <span>{agentRun.taskProgress.progress}%</span>
              </div>
              <div className="bloomery-chat-inspector-progress" aria-label={`${agentRun.taskProgress.progress}%`}>
                <span style={{ width: `${Math.max(0, Math.min(100, agentRun.taskProgress.progress))}%` }} />
              </div>
            </section>
          )}

          {agentRun.citationNumbers.length > 0 && (
            <section className="bloomery-chat-inspector-section" aria-labelledby="chat-evidence-heading">
              <div className="bloomery-chat-inspector-section-heading">
                <h4 id="chat-evidence-heading"><BookOpen size={15} aria-hidden="true" />{t("citationSection")}</h4>
              </div>
              <div className="bloomery-chat-inspector-citations">
                {agentRun.citationNumbers.map((number) => <span key={number}>[{number}]</span>)}
              </div>
            </section>
          )}
        </>
      )}
    </aside>
  );
}

export default function ChatView({
  conversations,
  selectedId,
  selectedConversation,
  messages,
  loading,
  loadingMessages,
  draft,
  pendingQuestion,
  agentRun,
  error,
  notice,
  onNewConversation,
  onSelectConversation,
  onDraftChange,
  onSubmit,
  onCancel,
  onResolvePermission,
  onExportConversation,
}: ChatViewProps) {
  const { t } = useLocale();

  return (
    <section className="bloomery-chat" aria-labelledby="chat-heading">
      <aside className="bloomery-chat-sidebar" aria-label={t("conversationList")}>
        <div className="bloomery-chat-sidebar-heading">
          <div><p className="bloomery-eyebrow">{t("localSessions")}</p><h1 id="chat-heading">{t("chatTitle")}</h1></div>
          <button type="button" className="bloomery-icon-button" onClick={onNewConversation} aria-label={t("newConversation")} title={t("newConversation")}><MessageSquarePlus size={17} aria-hidden="true" /></button>
        </div>
        <div className="bloomery-chat-session-list">
          {loading ? <div className="bloomery-chat-list-state"><LoaderCircle size={16} className="bloomery-spin" />{t("loading")}</div> : conversations.length === 0 ? (
            <div className="bloomery-chat-list-state">{t("noLocalSessions")}</div>
          ) : conversations.map((conversation) => (
            <button type="button" key={conversation.id} className={`bloomery-chat-session ${conversation.id === selectedId ? "is-active" : ""}`} onClick={() => onSelectConversation(conversation.id)}><span>{conversation.title}</span></button>
          ))}
        </div>
        <p className="bloomery-sidebar-footer">{t("chatSidebarFooter")}</p>
      </aside>

      <div className="bloomery-chat-main">
        <header className="bloomery-chat-header">
          <div><p className="bloomery-eyebrow">{t("steelRuntime")}</p><h2>{selectedConversation?.title ?? t("startSpecificQuestion")}</h2></div>
          {selectedConversation && (
            <div className="bloomery-chat-header-actions" aria-label={t("chatExportActions")}>
              <button
                type="button"
                className="bloomery-icon-button"
                data-testid="export-conversation-markdown"
                onClick={() => onExportConversation("markdown")}
                aria-label={t("chatExportMarkdown")}
                title={t("chatExportMarkdown")}
              >
                <Download size={17} aria-hidden="true" />
              </button>
              <button
                type="button"
                className="bloomery-icon-button"
                data-testid="export-conversation-json"
                onClick={() => onExportConversation("json")}
                aria-label={t("chatExportJson")}
                title={t("chatExportJson")}
              >
                <FileJson size={17} aria-hidden="true" />
              </button>
            </div>
          )}
          {agentRun && agentRun.conversationId === selectedId && (
            <div className="bloomery-chat-run-status" data-testid="agent-run-status" aria-live="polite"><span className="bloomery-state-dot" /><span>{agentStateLabel(agentRun.state, t)}</span>{agentRun.toolCalls.length > 0 && <span>{t("agentToolCount", { count: agentRun.toolCalls.length })}</span>}</div>
          )}
          <span className="bloomery-chat-runtime"><span className="bloomery-state-dot" />{t("localAgent")}</span>
        </header>

        {error && <div className="bloomery-knowledge-alert" role="alert">{error}</div>}
        {notice && <div className="bloomery-chat-export-notice" role="status">{notice}</div>}
        <div className="bloomery-chat-messages" aria-live="polite">
          {loadingMessages ? (
            <div className="bloomery-chat-empty"><LoaderCircle size={20} className="bloomery-spin" /><span>{t("loading")}</span></div>
          ) : messages.length === 0 && pendingQuestion === null ? (
            <div className="bloomery-chat-empty bloomery-chat-empty-large"><span className="bloomery-chat-empty-icon"><Sparkles size={22} /></span><strong>{t("startSpecificQuestion")}</strong><span>{t("exampleQuestion")}</span></div>
          ) : (
            <>
              {messages.map((message) => (
                <article className={`bloomery-chat-message ${isAssistant(message) ? "is-assistant" : "is-user"}`} key={message.id}>
                  <div className="bloomery-chat-message-meta">{isAssistant(message) ? <Bot size={15} aria-hidden="true" /> : <span>{t("me")}</span>}<span>{isAssistant(message) ? "Bloomery" : t("question")}</span></div>
                  {isAssistant(message) ? <><div className="bloomery-chat-answer ai-markdown-body"><AIAnswerRenderer answer={message.content} literatureResults={[]} /></div>{(() => { const evidence = messageEvidence(message); return evidence ? <CitationPanel auditId={evidence.evidencePackId} evidence={evidence.evidence} /> : null; })()}</> : <p>{message.content}</p>}
                </article>
              ))}
              {pendingQuestion && <>
                <article className="bloomery-chat-message is-user is-pending"><div className="bloomery-chat-message-meta"><span>{t("me")}</span><span>{t("question")}</span></div><p>{pendingQuestion}</p></article>
                <article className="bloomery-chat-message is-assistant is-streaming"><div className="bloomery-chat-message-meta"><Bot size={15} aria-hidden="true" /><span>Bloomery 路 {t("generating")}</span></div><div className="bloomery-chat-answer ai-markdown-body"><AIAnswerRenderer answer={agentRun?.assistantText || t("contextPreparing")} literatureResults={[]} /></div>{agentRun && agentRun.toolCalls.length > 0 && <div className="bloomery-chat-tool-trace" aria-label="Agent tools">{agentRun.toolCalls.map((tool) => <span key={tool.toolCallId}>{t("agentToolProgress", { name: tool.name, progress: tool.progress })}</span>)}</div>}</article>
              </>}
            </>
          )}
        </div>

        <form className="bloomery-chat-composer" onSubmit={onSubmit}>
          <textarea aria-label={t("inputMessage")} value={draft} onChange={(event) => onDraftChange(event.target.value)} placeholder={t("askPlaceholder")} rows={3} disabled={pendingQuestion !== null} />
          <div className="bloomery-chat-composer-footer"><span>{t("enterSend")}</span>{pendingQuestion ? <button type="button" className="bloomery-action-secondary" onClick={onCancel}><Square size={15} />{t("stopGenerating")}</button> : <button type="submit" className="bloomery-action-primary" disabled={!draft.trim()}><Send size={16} />{t("send")}</button>}</div>
        </form>
      </div>
      <RunInspector agentRun={agentRun && agentRun.conversationId === selectedId ? agentRun : null} onResolvePermission={onResolvePermission} />
      <span className="bloomery-chat-mobile-icon" aria-hidden="true"><PanelLeft size={17} /></span>
    </section>
  );
}
