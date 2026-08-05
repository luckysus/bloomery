import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  Bot,
  LoaderCircle,
  MessageSquarePlus,
  PanelLeft,
  Send,
  Square,
  Sparkles,
} from "lucide-react";
import AIAnswerRenderer from "../../components/answer/AnswerRenderer";
import { useLocale } from "../../i18n/locale";
import CitationPanel from "./CitationPanel";
import {
  desktop,
  type Conversation,
  type EvidenceItem,
  type Message,
} from "../../bridge/desktop";

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function isAssistant(message: Message) {
  return message.role === "agent" || message.role === "assistant";
}

function conversationTitle(message: string, fallback: string) {
  const title = message.trim().slice(0, 28);
  return title || fallback;
}

interface MessageEvidence {
  evidencePackId: string;
  evidence: EvidenceItem[];
}

function messageEvidence(message: Message): MessageEvidence | null {
  if (!isAssistant(message) || !message.response_json) return null;
  try {
    const response = JSON.parse(message.response_json) as {
      evidence_pack_id?: unknown;
      evidence?: unknown;
    };
    if (typeof response.evidence_pack_id !== "string" || !Array.isArray(response.evidence)) return null;
    return {
      evidencePackId: response.evidence_pack_id,
      evidence: response.evidence as EvidenceItem[],
    };
  } catch {
    return null;
  }
}

export default function ChatPage() {
  const { t } = useLocale();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [knowledgeBaseIds, setKnowledgeBaseIds] = useState<string[]>([]);
  const [draft, setDraft] = useState("");
  const [pendingQuestion, setPendingQuestion] = useState<string | null>(null);
  const [streamingAnswer, setStreamingAnswer] = useState("");
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMessages, setLoadingMessages] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === selectedId) ?? null,
    [conversations, selectedId],
  );

  const loadConversations = async () => {
    setLoading(true);
    try {
      const next = await desktop.listConversations();
      setConversations(next);
      setSelectedId((current) => current && next.some((conversation) => conversation.id === current)
        ? current
        : next[0]?.id ?? null);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    } finally {
      setLoading(false);
    }
  };

  const loadConversation = async (conversationId: string) => {
    setLoadingMessages(true);
    try {
      const [nextMessages, nextDraft] = await Promise.all([
        desktop.listMessages(conversationId),
        desktop.getConversationDraft(conversationId),
      ]);
      setMessages(nextMessages);
      setDraft(nextDraft);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    } finally {
      setLoadingMessages(false);
    }
  };

  useEffect(() => {
    void loadConversations();
    void desktop.listKnowledgeBases()
      .then((bases) => setKnowledgeBaseIds(bases.map((base) => base.id)))
      .catch((cause) => setError(errorMessage(cause, t("chatError"))));
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setMessages([]);
      setDraft("");
      return;
    }
    void loadConversation(selectedId);
  }, [selectedId]);

  useEffect(() => {
    if (!selectedId || loadingMessages || pendingQuestion !== null) return;
    const timer = window.setTimeout(() => {
      void desktop.saveConversationDraft(selectedId, draft).catch((cause) => setError(errorMessage(cause, t("chatError"))));
    }, 450);
    return () => window.clearTimeout(timer);
  }, [draft, loadingMessages, pendingQuestion, selectedId]);

  const createConversation = async () => {
    setError(null);
    try {
      const created = await desktop.createConversation(t("newConversation"));
      setConversations((current) => [created, ...current]);
      setSelectedId(created.id);
      setMessages([]);
      setDraft("");
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const refreshConversation = async (conversationId: string) => {
    const [nextMessages, nextConversations] = await Promise.all([
      desktop.listMessages(conversationId),
      desktop.listConversations(),
    ]);
    setMessages(nextMessages);
    setConversations(nextConversations);
  };

  const submitMessage = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const question = draft.trim();
    if (!question || pendingQuestion !== null) return;

    setError(null);
    let conversationId = selectedId;
    try {
      if (!conversationId) {
        const created = await desktop.createConversation(conversationTitle(question, t("newConversation")));
        conversationId = created.id;
        setConversations((current) => [created, ...current]);
        setSelectedId(created.id);
      }

      const runId = crypto.randomUUID();
      setPendingQuestion(question);
      setStreamingAnswer("");
      setActiveRunId(runId);
      setDraft("");

      let evidencePackId: string | undefined;
      if (knowledgeBaseIds.length > 0) {
        try {
          const evidencePack = await desktop.queryLocalKnowledge({
            query: question,
            knowledge_base_ids: knowledgeBaseIds,
          });
          evidencePackId = evidencePack.id;
        } catch (cause) {
          setError(errorMessage(cause, t("chatError")));
        }
      }

      let receivedDelta = false;
      const unlisten = await desktop.listenDesktopAgentDeltas((delta) => {
        if (delta.run_id !== runId) return;
        receivedDelta = true;
        setStreamingAnswer((current) => current + delta.delta);
      });
      try {
        const response = await desktop.desktopAgentChat({
            sessionId: conversationId,
            message: question,
            runId,
            evidencePackId,
          });
        if (!receivedDelta && response.answer) setStreamingAnswer(response.answer);
        await refreshConversation(conversationId);
      } finally {
        unlisten();
      }
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
      if (conversationId) {
        await refreshConversation(conversationId).catch(() => undefined);
      }
    } finally {
      setPendingQuestion(null);
      setStreamingAnswer("");
      setActiveRunId(null);
    }
  };

  const cancelRun = async () => {
    if (!activeRunId) return;
    try {
      await desktop.cancelDesktopRun(activeRunId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  return (
    <section className="bloomery-chat" aria-labelledby="chat-heading">
      <aside className="bloomery-chat-sidebar" aria-label={t("conversationList")}>
        <div className="bloomery-chat-sidebar-heading">
          <div>
            <p className="bloomery-eyebrow">{t("localSessions")}</p>
            <h1 id="chat-heading">{t("chatTitle")}</h1>
          </div>
          <button type="button" className="bloomery-icon-button" onClick={() => void createConversation()} aria-label={t("newConversation")} title={t("newConversation")}>
            <MessageSquarePlus size={17} aria-hidden="true" />
          </button>
        </div>
        <div className="bloomery-chat-session-list">
          {loading ? <div className="bloomery-chat-list-state"><LoaderCircle size={16} className="bloomery-spin" />{t("loading")}</div> : conversations.length === 0 ? (
            <div className="bloomery-chat-list-state">{t("noLocalSessions")}</div>
          ) : conversations.map((conversation) => (
            <button
              type="button"
              key={conversation.id}
              className={`bloomery-chat-session ${conversation.id === selectedId ? "is-active" : ""}`}
              onClick={() => setSelectedId(conversation.id)}
            >
              <span>{conversation.title}</span>
            </button>
          ))}
        </div>
        <p className="bloomery-sidebar-footer">{t("chatSidebarFooter")}</p>
      </aside>

      <div className="bloomery-chat-main">
        <header className="bloomery-chat-header">
          <div>
            <p className="bloomery-eyebrow">{t("steelRuntime")}</p>
            <h2>{selectedConversation?.title ?? t("startSpecificQuestion")}</h2>
          </div>
          <span className="bloomery-chat-runtime"><span className="bloomery-state-dot" />{t("localAgent")}</span>
        </header>

        {error && <div className="bloomery-knowledge-alert" role="alert">{error}</div>}

        <div className="bloomery-chat-messages" aria-live="polite">
          {loadingMessages ? (
            <div className="bloomery-chat-empty"><LoaderCircle size={20} className="bloomery-spin" /><span>{t("loading")}</span></div>
          ) : messages.length === 0 && pendingQuestion === null ? (
            <div className="bloomery-chat-empty bloomery-chat-empty-large">
              <span className="bloomery-chat-empty-icon"><Sparkles size={22} /></span>
              <strong>{t("startSpecificQuestion")}</strong>
              <span>{t("exampleQuestion")}</span>
            </div>
          ) : (
            <>
              {messages.map((message) => (
                <article className={`bloomery-chat-message ${isAssistant(message) ? "is-assistant" : "is-user"}`} key={message.id}>
                  <div className="bloomery-chat-message-meta">
                    {isAssistant(message) ? <Bot size={15} aria-hidden="true" /> : <span>{t("me")}</span>}
                    <span>{isAssistant(message) ? "Bloomery" : t("question")}</span>
                  </div>
                  {isAssistant(message) ? (
                    <>
                      <div className="bloomery-chat-answer ai-markdown-body"><AIAnswerRenderer answer={message.content} literatureResults={[]} /></div>
                      {(() => {
                        const evidence = messageEvidence(message);
                        return evidence ? <CitationPanel auditId={evidence.evidencePackId} evidence={evidence.evidence} /> : null;
                      })()}
                    </>
                  ) : <p>{message.content}</p>}
                </article>
              ))}
              {pendingQuestion && (
                <>
                  <article className="bloomery-chat-message is-user is-pending">
                    <div className="bloomery-chat-message-meta"><span>{t("me")}</span><span>{t("question")}</span></div>
                    <p>{pendingQuestion}</p>
                  </article>
                  <article className="bloomery-chat-message is-assistant is-streaming">
                    <div className="bloomery-chat-message-meta"><Bot size={15} aria-hidden="true" /><span>Bloomery · {t("generating")}</span></div>
                    <div className="bloomery-chat-answer ai-markdown-body"><AIAnswerRenderer answer={streamingAnswer || t("contextPreparing")} literatureResults={[]} /></div>
                  </article>
                </>
              )}
            </>
          )}
        </div>

        <form className="bloomery-chat-composer" onSubmit={submitMessage}>
          <textarea
            aria-label={t("inputMessage")}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={t("askPlaceholder")}
            rows={3}
            disabled={pendingQuestion !== null}
          />
          <div className="bloomery-chat-composer-footer">
            <span>{t("enterSend")}</span>
            {pendingQuestion ? (
              <button type="button" className="bloomery-action-secondary" onClick={() => void cancelRun()}><Square size={15} />{t("stopGenerating")}</button>
            ) : (
              <button type="submit" className="bloomery-action-primary" disabled={!draft.trim()}><Send size={16} />{t("send")}</button>
            )}
          </div>
        </form>
      </div>
      <span className="bloomery-chat-mobile-icon" aria-hidden="true"><PanelLeft size={17} /></span>
    </section>
  );
}
