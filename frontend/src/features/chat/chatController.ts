import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { useLocale } from "../../i18n/locale";
import {
  desktop,
  type Conversation,
  type ConversationExportFormat,
  type HistoryHit,
  type LocalAgentAttachment,
  type Message,
  type ProviderProfileResponse,
  type RecoveredRun,
} from "../../bridge/desktop";
import type { PermissionDecision } from "../../bridge/generated/protocol";
import { createAgentRunView, reduceAgentEvent, reduceAgentEvents, type AgentRunView } from "./agentEvents";

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

function exportFileName(title: string, extension: string) {
  const safeTitle = title
    .trim()
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-")
    .slice(0, 80)
    .trim();
  return `${safeTitle || "bloomery-conversation"}.${extension}`;
}

function shouldRunSmartSearch(question: string) {
  const value = question.trim();
  if (!value) return false;
  const compact = value.toLocaleLowerCase().replace(/\s+/g, "");
  if (["你好", "您好", "hello", "hi", "hey", "在吗", "谢谢", "thanks"].includes(compact)) {
    return false;
  }
  const cjkCount = Array.from(value).filter((character) => {
    const code = character.charCodeAt(0);
    return (code >= 0x3400 && code <= 0x9fff) || (code >= 0xf900 && code <= 0xfaff);
  }).length;
  const asciiTokens = value.match(/[a-z0-9][a-z0-9._/-]*/gi)?.length ?? 0;
  return cjkCount >= 4 || asciiTokens >= 2 || /钢|铁|材料|牌号|屈服|抗拉|延伸|成分|工艺|标准|文献|知识库|q\d/i.test(value);
}

export interface ChatControllerProps {
  conversations: Conversation[];
  selectedId: string | null;
  selectedConversation: Conversation | null;
  messages: Message[];
  loading: boolean;
  loadingMessages: boolean;
  draft: string;
  pendingQuestion: string | null;
  agentRun: AgentRunView | null;
  recovery: RecoveredRun | null;
  chatProfiles: ProviderProfileResponse[];
  activeChatProfileId: string | null;
  smartSearchEnabled: boolean;
  attachments: LocalAgentAttachment[];
  error: string | null;
  notice: string | null;
  onNewConversation: () => void;
  onSelectConversation: (id: string) => void;
  onDraftChange: (value: string) => void;
  onAttachmentsChange: (value: LocalAgentAttachment[]) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCancel: () => void;
  onSteer: (message: string) => void;
  onFollowUp: (message: string) => void;
  onRetry: () => void;
  onResume: () => void;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
  onExportConversation: (format: ConversationExportFormat) => void;
  onRenameConversation: (conversationId: string, title: string) => void;
  onToggleConversationPinned: (conversation: Conversation) => void;
  onArchiveConversation: (conversationId: string) => void;
  onDeleteConversation: (conversationId: string) => void;
  onSearchHistory: (query: string) => Promise<HistoryHit[]>;
  onSelectChatProfile: (profileId: string) => void;
  onToggleSmartSearch: () => void;
}

export function useChatController(): ChatControllerProps {
  const { t } = useLocale();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [knowledgeBaseIds, setKnowledgeBaseIds] = useState<string[]>([]);
  const [chatProfiles, setChatProfiles] = useState<ProviderProfileResponse[]>([]);
  const [activeChatProfileId, setActiveChatProfileId] = useState<string | null>(null);
  const [smartSearchEnabled, setSmartSearchEnabled] = useState(false);
  const [attachments, setAttachments] = useState<LocalAgentAttachment[]>([]);
  const [draft, setDraft] = useState("");
  const [pendingQuestion, setPendingQuestion] = useState<string | null>(null);
  const [agentRun, setAgentRun] = useState<AgentRunView | null>(null);
  const [recoveredRuns, setRecoveredRuns] = useState<RecoveredRun[]>([]);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMessages, setLoadingMessages] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const agentViews = useRef(new Map<string, AgentRunView>());
  const replayingRuns = useRef(new Set<string>());
  const recoveredRunsRef = useRef<RecoveredRun[]>([]);

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
      const recovered = recoveredRunsRef.current.find((candidate) => candidate.run.conversation_id === conversationId);
      const runId = [...nextMessages].reverse().map(messageRunId).find((value): value is string => value !== null)
        ?? recovered?.run.id;
      if (runId) {
        const events = await desktop.replayAgentRun(runId);
        if (events.length > 0) {
          const view = reduceAgentEvents(createAgentRunView(runId, conversationId), events);
          agentViews.current.set(`${conversationId}:${runId}`, view);
          setAgentRun(view);
        } else if (recovered) {
          const view = { ...createAgentRunView(runId, conversationId), state: recovered.run.state };
          agentViews.current.set(`${conversationId}:${runId}`, view);
          setAgentRun(view);
        }
      }
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    } finally {
      setLoadingMessages(false);
    }
  };

  useEffect(() => {
    let mounted = true;
    void Promise.all([
      desktop.listKnowledgeBases(),
      desktop.listProviderProfiles(),
      desktop.recoverAgentRuns().catch(() => [] as RecoveredRun[]),
    ])
      .then(async ([bases, profiles, recoveries]) => {
        if (!mounted) return;
        setKnowledgeBaseIds(bases.map((base) => base.id));
        const available = profiles.filter((profile) => profile.enabled && profile.model_id && ["deepseek", "open_ai_compatible", "ollama"].includes(profile.kind));
        setChatProfiles(available);
        setActiveChatProfileId((current) => current && available.some((profile) => profile.id === current)
          ? current
          : available[0]?.id ?? null);
        recoveredRunsRef.current = recoveries;
        setRecoveredRuns(recoveries);
        await loadConversations();
      })
      .catch((cause) => {
        if (mounted) setError(errorMessage(cause, t("chatError")));
      });
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    let mounted = true;
    let dispose: (() => void) | undefined;
    const handleEvent = (event: Parameters<Parameters<typeof desktop.listenAgentEvents>[0]>[0]) => {
      if (!mounted || (selectedId && event.conversation_id !== selectedId)) return;
      const key = `${event.conversation_id}:${event.run_id}`;
      const current = agentViews.current.get(key) ?? createAgentRunView(event.run_id, event.conversation_id);
      const previousSequence = current.sequence;
      const next = reduceAgentEvent(current, event);
      agentViews.current.set(key, next);
      setAgentRun((visible) => visible?.runId === event.run_id && visible.conversationId === event.conversation_id
        ? next
        : visible ?? next);
      if (event.sequence > previousSequence + 1 && !replayingRuns.current.has(key)) {
        replayingRuns.current.add(key);
        void desktop.replayAgentRun(event.run_id, previousSequence)
          .then((events) => {
            if (!mounted) return;
            const replayed = reduceAgentEvents(agentViews.current.get(key) ?? current, events);
            agentViews.current.set(key, replayed);
            setAgentRun((visible) => visible?.runId === event.run_id && visible.conversationId === event.conversation_id
              ? replayed
              : visible ?? replayed);
          })
          .catch((cause) => {
            if (mounted) setError(errorMessage(cause, t("chatError")));
          })
          .finally(() => replayingRuns.current.delete(key));
      }
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
    setNotice(null);
    try {
      const created = await desktop.createConversation(t("newConversation"));
      setConversations((current) => [created, ...current]);
      setSelectedId(created.id);
      setMessages([]);
      setDraft("");
      setAttachments([]);
      setAgentRun(null);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const exportSelectedConversation = async (format: ConversationExportFormat) => {
    if (!selectedConversation) return;
    setError(null);
    setNotice(null);
    const extension = format === "json" ? "json" : "md";
    try {
      const selected = await desktop.saveFileDialog({
        title: t(format === "json" ? "chatExportJson" : "chatExportMarkdown"),
        defaultPath: exportFileName(selectedConversation.title, extension),
        filters: [{
          name: t(format === "json" ? "chatExportJsonFile" : "chatExportMarkdownFile"),
          extensions: [extension],
        }],
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      await desktop.exportConversation(selectedConversation.id, selected, format);
      setNotice(t("chatExported"));
    } catch (cause) {
      setError(errorMessage(cause, t("chatExportError")));
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
    if ((!question && attachments.length === 0) || pendingQuestion !== null) return;
    setError(null);
    let conversationId = selectedId;
    try {
      if (!conversationId) {
        const created = await desktop.createConversation(conversationTitle(question || "图片分析", t("newConversation")));
        conversationId = created.id;
        setConversations((current) => [created, ...current]);
        setSelectedId(created.id);
      }
      const runId = crypto.randomUUID();
      const submittedMessage = question || "请分析附加图片";
      const submittedAttachments = attachments;
      setPendingQuestion(submittedMessage);
      setAgentRun(createAgentRunView(runId, conversationId));
      setActiveRunId(runId);
      setDraft("");
      setAttachments([]);

      let evidencePackId: string | undefined;
      if (smartSearchEnabled && shouldRunSmartSearch(question) && knowledgeBaseIds.length > 0) {
        try {
          const evidencePack = await desktop.queryLocalKnowledge({ query: question, knowledge_base_ids: knowledgeBaseIds });
          evidencePackId = evidencePack.id;
        } catch (cause) {
          setError(errorMessage(cause, t("chatError")));
        }
      }
      const response = await desktop.desktopAgentChat({
        sessionId: conversationId,
        message: submittedMessage,
        runId,
        evidencePackId,
        smartSearchEnabled,
        attachments: submittedAttachments,
      });
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
      await desktop.cancelAgentRun(activeRunId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const steerRun = async (message: string) => {
    const value = message.trim();
    if (!activeRunId || !value) return;
    try {
      await desktop.steerAgentRun(activeRunId, value);
      setDraft("");
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const followUpRun = async (message: string) => {
    const value = message.trim();
    if (!activeRunId || !value) return;
    try {
      await desktop.followUpAgentRun(activeRunId, value);
      setDraft("");
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const retryRun = async () => {
    const sourceRunId = agentRun?.runId;
    const source = recoveredRunsRef.current.find((candidate) => candidate.run.id === sourceRunId);
    const sourceMessage = source
      ? messages.find((message) => message.id === source.run.user_message_id)
      : [...messages].reverse().find((message) => message.role === "user");
    if (!selectedId || !sourceRunId || !sourceMessage || pendingQuestion !== null) return;
    const runId = crypto.randomUUID();
    setError(null);
    setPendingQuestion(sourceMessage.content);
    setAgentRun(createAgentRunView(runId, selectedId));
    setActiveRunId(runId);
    try {
      const response = await desktop.desktopAgentChat({
        sessionId: selectedId,
        message: sourceMessage.content,
        runId,
        smartSearchEnabled,
      });
      setAgentRun((current) => current?.runId === runId && !current.assistantText && response.answer
        ? { ...current, assistantText: response.answer }
        : current);
      await refreshConversation(selectedId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
      await refreshConversation(selectedId).catch(() => undefined);
    } finally {
      setPendingQuestion(null);
      setActiveRunId(null);
    }
  };

  const resumeRun = async () => {
    if (!agentRun) return;
    try {
      const recoveries = await desktop.recoverAgentRuns();
      recoveredRunsRef.current = recoveries;
      setRecoveredRuns(recoveries);
      const events = await desktop.replayAgentRun(agentRun.runId, agentRun.sequence);
      if (events.length > 0) {
        const key = `${agentRun.conversationId}:${agentRun.runId}`;
        const view = reduceAgentEvents(agentViews.current.get(key) ?? agentRun, events);
        agentViews.current.set(key, view);
        setAgentRun(view);
      }
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const resolvePermission = async (permissionId: string, decision: PermissionDecision) => {
    try {
      await desktop.resolveAgentPermission(permissionId, decision);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const reloadAfter = async (action: () => Promise<void>) => {
    try {
      await action();
      await loadConversations();
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  const renameConversation = async (conversationId: string, title: string) => {
    const nextTitle = title.trim();
    if (nextTitle) await reloadAfter(() => desktop.updateConversationTitle(conversationId, nextTitle));
  };

  const toggleConversationPinned = async (conversation: Conversation) => {
    await reloadAfter(() => desktop.updateConversationPinned(conversation.id, !conversation.pinned));
  };

  const archiveConversation = async (conversationId: string) => {
    await reloadAfter(() => desktop.archiveConversation(conversationId));
  };

  const deleteConversation = async (conversationId: string) => {
    if (window.confirm("确定删除这个本地对话吗？")) {
      await reloadAfter(() => desktop.deleteConversationLocal(conversationId));
    }
  };

  const searchHistory = useCallback(async (query: string): Promise<HistoryHit[]> => {
    if (!query.trim()) return [];
    try {
      return await desktop.searchHistory({ query, limit: 12 });
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
      return [];
    }
  }, [t]);

  const selectChatProfile = async (profileId: string) => {
    try {
      await desktop.setDefaultProvider("chat", profileId);
      setActiveChatProfileId(profileId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    }
  };

  return {
    conversations,
    selectedId,
    selectedConversation,
    messages,
    loading,
    loadingMessages,
    draft,
    pendingQuestion,
    agentRun,
    recovery: recoveredRuns.find((candidate) => candidate.run.id === agentRun?.runId) ?? null,
    chatProfiles,
    activeChatProfileId,
    smartSearchEnabled,
    attachments,
    error,
    notice,
    onNewConversation: () => void createConversation(),
    onSelectConversation: setSelectedId,
    onDraftChange: setDraft,
    onAttachmentsChange: setAttachments,
    onSubmit: submitMessage,
    onCancel: () => void cancelRun(),
    onSteer: (message) => void steerRun(message),
    onFollowUp: (message) => void followUpRun(message),
    onRetry: () => void retryRun(),
    onResume: () => void resumeRun(),
    onResolvePermission: (permissionId, decision) => void resolvePermission(permissionId, decision),
    onExportConversation: (format) => void exportSelectedConversation(format),
    onRenameConversation: (conversationId, title) => void renameConversation(conversationId, title),
    onToggleConversationPinned: (conversation) => void toggleConversationPinned(conversation),
    onArchiveConversation: (conversationId) => void archiveConversation(conversationId),
    onDeleteConversation: (conversationId) => void deleteConversation(conversationId),
    onSearchHistory: searchHistory,
    onSelectChatProfile: (profileId) => void selectChatProfile(profileId),
    onToggleSmartSearch: () => setSmartSearchEnabled((enabled) => !enabled),
  };
}
