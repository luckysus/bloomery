import type { FormEvent } from "react";
import {
  Bot,
  LoaderCircle,
  MessageSquarePlus,
  PanelLeft,
  Send,
  Sparkles,
  Square,
} from "lucide-react";
import AIAnswerRenderer from "../../components/answer/AnswerRenderer";
import { useLocale } from "../../i18n/locale";
import { type Conversation, type EvidenceItem, type Message } from "../../bridge/desktop";
import type { AgentRunState } from "../../bridge/generated/protocol";
import CitationPanel from "./CitationPanel";
import type { AgentRunView } from "./agentEvents";

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
  onNewConversation: () => void;
  onSelectConversation: (id: string) => void;
  onDraftChange: (value: string) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCancel: () => void;
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
  onNewConversation,
  onSelectConversation,
  onDraftChange,
  onSubmit,
  onCancel,
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
          {agentRun && agentRun.conversationId === selectedId && (
            <div className="bloomery-chat-run-status" data-testid="agent-run-status" aria-live="polite"><span className="bloomery-state-dot" /><span>{agentStateLabel(agentRun.state, t)}</span>{agentRun.toolCalls.length > 0 && <span>{t("agentToolCount", { count: agentRun.toolCalls.length })}</span>}</div>
          )}
          <span className="bloomery-chat-runtime"><span className="bloomery-state-dot" />{t("localAgent")}</span>
        </header>

        {error && <div className="bloomery-knowledge-alert" role="alert">{error}</div>}
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
      <span className="bloomery-chat-mobile-icon" aria-hidden="true"><PanelLeft size={17} /></span>
    </section>
  );
}
