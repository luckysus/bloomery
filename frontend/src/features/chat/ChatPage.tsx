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
import { desktop, type Conversation, type Message } from "../../bridge/desktop";

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "本地对话失败，请检查模型配置后重试。";
}

function isAssistant(message: Message) {
  return message.role === "agent" || message.role === "assistant";
}

function conversationTitle(message: string) {
  const title = message.trim().slice(0, 28);
  return title || "新对话";
}

export default function ChatPage() {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
    } finally {
      setLoadingMessages(false);
    }
  };

  useEffect(() => {
    void loadConversations();
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
      void desktop.saveConversationDraft(selectedId, draft).catch((cause) => setError(errorMessage(cause)));
    }, 450);
    return () => window.clearTimeout(timer);
  }, [draft, loadingMessages, pendingQuestion, selectedId]);

  const createConversation = async () => {
    setError(null);
    try {
      const created = await desktop.createConversation("新对话");
      setConversations((current) => [created, ...current]);
      setSelectedId(created.id);
      setMessages([]);
      setDraft("");
    } catch (cause) {
      setError(errorMessage(cause));
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
        const created = await desktop.createConversation(conversationTitle(question));
        conversationId = created.id;
        setConversations((current) => [created, ...current]);
        setSelectedId(created.id);
      }

      const runId = crypto.randomUUID();
      setPendingQuestion(question);
      setStreamingAnswer("");
      setActiveRunId(runId);
      setDraft("");

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
        });
        if (!receivedDelta && response.answer) setStreamingAnswer(response.answer);
        await refreshConversation(conversationId);
      } finally {
        unlisten();
      }
    } catch (cause) {
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
    }
  };

  return (
    <section className="bloomery-chat" aria-labelledby="chat-heading">
      <aside className="bloomery-chat-sidebar" aria-label="对话列表">
        <div className="bloomery-chat-sidebar-heading">
          <div>
            <p className="bloomery-eyebrow">LOCAL SESSIONS</p>
            <h1 id="chat-heading">对话</h1>
          </div>
          <button type="button" className="bloomery-icon-button" onClick={() => void createConversation()} aria-label="新建对话" title="新建对话">
            <MessageSquarePlus size={17} aria-hidden="true" />
          </button>
        </div>
        <div className="bloomery-chat-session-list">
          {loading ? <div className="bloomery-chat-list-state"><LoaderCircle size={16} className="bloomery-spin" />正在读取</div> : conversations.length === 0 ? (
            <div className="bloomery-chat-list-state">还没有本地会话</div>
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
        <p className="bloomery-sidebar-footer">本地保存 · 上下文可追溯</p>
      </aside>

      <div className="bloomery-chat-main">
        <header className="bloomery-chat-header">
          <div>
            <p className="bloomery-eyebrow">STEEL AGENT / LOCAL RUNTIME</p>
            <h2>{selectedConversation?.title ?? "开始一次钢铁领域工作"}</h2>
          </div>
          <span className="bloomery-chat-runtime"><span className="bloomery-state-dot" />本地智能体</span>
        </header>

        {error && <div className="bloomery-knowledge-alert" role="alert">{error}</div>}

        <div className="bloomery-chat-messages" aria-live="polite">
          {loadingMessages ? (
            <div className="bloomery-chat-empty"><LoaderCircle size={20} className="bloomery-spin" /><span>正在读取会话</span></div>
          ) : messages.length === 0 && pendingQuestion === null ? (
            <div className="bloomery-chat-empty bloomery-chat-empty-large">
              <span className="bloomery-chat-empty-icon"><Sparkles size={22} /></span>
              <strong>从一个具体问题开始</strong>
              <span>例如：比较 Q345B 与 Q355B 的屈服强度要求，并指出适用标准。</span>
            </div>
          ) : (
            <>
              {messages.map((message) => (
                <article className={`bloomery-chat-message ${isAssistant(message) ? "is-assistant" : "is-user"}`} key={message.id}>
                  <div className="bloomery-chat-message-meta">
                    {isAssistant(message) ? <Bot size={15} aria-hidden="true" /> : <span>我</span>}
                    <span>{isAssistant(message) ? "Bloomery" : "提问"}</span>
                  </div>
                  {isAssistant(message) ? (
                    <div className="bloomery-chat-answer ai-markdown-body"><AIAnswerRenderer answer={message.content} literatureResults={[]} /></div>
                  ) : <p>{message.content}</p>}
                </article>
              ))}
              {pendingQuestion && (
                <>
                  <article className="bloomery-chat-message is-user is-pending">
                    <div className="bloomery-chat-message-meta"><span>我</span><span>提问</span></div>
                    <p>{pendingQuestion}</p>
                  </article>
                  <article className="bloomery-chat-message is-assistant is-streaming">
                    <div className="bloomery-chat-message-meta"><Bot size={15} aria-hidden="true" /><span>Bloomery · 正在生成</span></div>
                    <div className="bloomery-chat-answer ai-markdown-body"><AIAnswerRenderer answer={streamingAnswer || "正在整理本地上下文…"} literatureResults={[]} /></div>
                  </article>
                </>
              )}
            </>
          )}
        </div>

        <form className="bloomery-chat-composer" onSubmit={submitMessage}>
          <textarea
            aria-label="输入消息"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="询问钢材、工艺、标准或实验数据…"
            rows={3}
            disabled={pendingQuestion !== null}
          />
          <div className="bloomery-chat-composer-footer">
            <span>Enter 发送 · 本地上下文优先</span>
            {pendingQuestion ? (
              <button type="button" className="bloomery-action-secondary" onClick={() => void cancelRun()}><Square size={15} />停止生成</button>
            ) : (
              <button type="submit" className="bloomery-action-primary" disabled={!draft.trim()}><Send size={16} />发送</button>
            )}
          </div>
        </form>
      </div>
      <span className="bloomery-chat-mobile-icon" aria-hidden="true"><PanelLeft size={17} /></span>
    </section>
  );
}
