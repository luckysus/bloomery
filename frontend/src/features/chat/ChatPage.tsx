import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useLocale } from "../../i18n/locale";
import { desktop, type Conversation, type Message } from "../../bridge/desktop";
import { createAgentRunView, reduceAgentEvent, reduceAgentEvents, type AgentRunView } from "./agentEvents";
import ChatView from "./ChatView";

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function conversationTitle(message: string, fallback: string) {
  const title = message.trim().slice(0, 28);
  return title || fallback;
}

function messageRunId(message: Message) {
  if (!message.response_json || !["agent", "assistant"].includes(message.role)) return null;
  try {
    const response = JSON.parse(message.response_json) as { run_id?: unknown };
    return typeof response.run_id === "string" && response.run_id.trim() ? response.run_id : null;
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
  const [agentRun, setAgentRun] = useState<AgentRunView | null>(null);
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
      setAgentRun(null);
      const runId = [...nextMessages].reverse().map(messageRunId).find((value): value is string => value !== null);
      if (runId) {
        const events = await desktop.replayAgentRun(runId);
        if (events.length > 0) setAgentRun(reduceAgentEvents(createAgentRunView(runId, conversationId), events));
      }
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
    let mounted = true;
    let dispose: (() => void) | undefined;
    const handleEvent = (event: Parameters<Parameters<typeof desktop.listenAgentEvents>[0]>[0]) => {
      if (!mounted || (selectedId && event.conversation_id !== selectedId)) return;
      setAgentRun((current) => {
        const view = current?.runId === event.run_id && current.conversationId === event.conversation_id
          ? current
          : createAgentRunView(event.run_id, event.conversation_id);
        return reduceAgentEvent(view, event);
      });
    };
    void desktop.listenAgentEvents(handleEvent)
      .then((unlisten) => {
        if (mounted) dispose = unlisten;
        else unlisten();
      })
      .catch((cause) => {
        if (mounted) setError(errorMessage(cause, t("chatError")));
      });
    return () => {
      mounted = false;
      dispose?.();
    };
  }, [selectedId, t]);

  useEffect(() => {
    if (!selectedId) {
      setMessages([]);
      setDraft("");
      setAgentRun(null);
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
      setAgentRun(null);
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
      setAgentRun(createAgentRunView(runId, conversationId));
      setActiveRunId(runId);
      setDraft("");

      let evidencePackId: string | undefined;
      if (knowledgeBaseIds.length > 0) {
        try {
          const evidencePack = await desktop.queryLocalKnowledge({ query: question, knowledge_base_ids: knowledgeBaseIds });
          evidencePackId = evidencePack.id;
        } catch (cause) {
          setError(errorMessage(cause, t("chatError")));
        }
      }

      const response = await desktop.desktopAgentChat({ sessionId: conversationId, message: question, runId, evidencePackId });
      setAgentRun((current) => {
        if (!current || current.runId !== runId || current.assistantText || !response.answer) return current;
        return { ...current, assistantText: response.answer };
      });
      await refreshConversation(conversationId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
      if (conversationId) await refreshConversation(conversationId).catch(() => undefined);
    } finally {
      setPendingQuestion(null);
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
    <ChatView
      conversations={conversations}
      selectedId={selectedId}
      selectedConversation={selectedConversation}
      messages={messages}
      loading={loading}
      loadingMessages={loadingMessages}
      draft={draft}
      pendingQuestion={pendingQuestion}
      agentRun={agentRun}
      error={error}
      onNewConversation={() => void createConversation()}
      onSelectConversation={(id) => setSelectedId(id)}
      onDraftChange={setDraft}
      onSubmit={submitMessage}
      onCancel={() => void cancelRun()}
    />
  );
}
