import { createContext, createElement, useCallback, useContext, useEffect, useMemo, useRef, type FormEvent, type ReactNode } from "react";
import { useLocale } from "../../i18n/locale";
import {
  desktop,
  type Conversation,
  type ConversationExportFormat,
  type HistoryHit,
  type KnowledgeBaseRecord,
  type PostgresKnowledgeSearchFilters,
  type LocalAgentAttachment,
  type Message,
  type AgentProfileSummary,
  type EvidenceItem,
  type ProviderProfileResponse,
  type RecoveredRun,
} from "../../bridge/desktop";
import type { PermissionDecision } from "../../bridge/generated/protocol";
import { createAgentRunView, reduceAgentEvent, reduceAgentEvents, type AgentRunView } from "./agentEvents";
import { useAppearanceSettings } from "../../settings/appearance";
import { useAgentStore } from "../../stores/agentStore";
import { useChatStore } from "../../stores/chatStore";
import { useKnowledgeStore } from "../../stores/knowledgeStore";
import { useModelStore } from "../../stores/modelStore";

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
  return `${safeTitle || "suna-conversation"}.${extension}`;
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

function parseKnowledgePreferences(raw: string | null) {
  try {
    const value = JSON.parse(raw ?? "{}") as Record<string, unknown>;
    const topK = typeof value.top_k === "number" && Number.isFinite(value.top_k)
      ? Math.min(50, Math.max(1, Math.round(value.top_k)))
      : 8;
    const similarityThreshold = typeof value.similarity_threshold === "number" && Number.isFinite(value.similarity_threshold)
      ? Math.min(1, Math.max(0, value.similarity_threshold))
      : 0.7;
    const rerankerEnabled = value.reranker_enabled !== false;
    return {
      defaultKnowledgeBase: typeof value.default_knowledge_base === "string" ? value.default_knowledge_base.trim() : "",
      citationsEnabled: value.citations_enabled !== false,
      autoRetrieve: value.auto_retrieve === true,
      retrievalOptions: {
        lexical_limit: topK * 3,
        dense_limit: topK * 3,
        candidate_limit: topK,
        rerank_limit: rerankerEnabled ? topK : 0,
        similarity_threshold: similarityThreshold,
        degradation_policy: value.degradation_policy === "strict" ? "strict" : "fallback",
      },
    };
  } catch {
    return { defaultKnowledgeBase: "", citationsEnabled: true, autoRetrieve: false, retrievalOptions: { degradation_policy: "fallback" } };
  }
}

function notifyRunResult(title: string, body: string) {
  if (typeof document === "undefined" || document.visibilityState !== "hidden") return;
  if (typeof Notification === "undefined" || Notification.permission !== "granted") return;
  try {
    new Notification(title, { body });
  } catch {
    // Browser previews and older desktop shells may not expose notifications.
  }
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
  agentProfiles: AgentProfileSummary[];
  activeAgentId: string | null;
  knowledgeBases: KnowledgeBaseRecord[];
  selectedKnowledgeBaseIds: string[];
  knowledgeFilters: PostgresKnowledgeSearchFilters;
  streamingCitations: EvidenceItem[];
  autoKnowledgeSearchEnabled: boolean;
  smartSearchEnabled: boolean;
  attachments: LocalAgentAttachment[];
  error: string | null;
  notice: string | null;
  onNewConversation: (initialDraft?: string) => void;
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
  onSelectAgent: (agentId: string | null) => void;
  onSelectKnowledgeBases: (knowledgeBaseIds: string[]) => void;
  onSelectKnowledgeFilters: (filters: PostgresKnowledgeSearchFilters) => void;
  onToggleSmartSearch: () => void;
}

const ChatControllerContext = createContext<ChatControllerProps | null>(null);

export function ChatControllerProvider({ children }: { children: ReactNode }) {
  const controller = useChatController();
  return createElement(ChatControllerContext.Provider, { value: controller }, children);
}

export function useChatControllerContext() {
  return useContext(ChatControllerContext) ?? useChatController();
}

export function useChatController(): ChatControllerProps {
  const { t } = useLocale();
  const { preferences, loaded: appearanceLoaded } = useAppearanceSettings();
  // 第 77 章：状态由 Zustand store 承载。setter 保持 useState 的更新器语义，
  // 因此本文件其余逻辑与所有消费方都无需改动。
  const conversations = useChatStore((state) => state.conversations);
  const selectedId = useChatStore((state) => state.selectedId);
  const messages = useChatStore((state) => state.messages);
  const draft = useChatStore((state) => state.draft);
  const pendingQuestion = useChatStore((state) => state.pendingQuestion);
  const streamingCitations = useChatStore((state) => state.streamingCitations);
  const attachments = useChatStore((state) => state.attachments);
  const loading = useChatStore((state) => state.loading);
  const loadingMessages = useChatStore((state) => state.loadingMessages);
  const error = useChatStore((state) => state.error);
  const notice = useChatStore((state) => state.notice);
  const smartSearchEnabled = useChatStore((state) => state.smartSearchEnabled);
  const autoSearchEnabled = useChatStore((state) => state.autoSearchEnabled);

  const agentRun = useAgentStore((state) => state.agentRun);
  const recoveredRuns = useAgentStore((state) => state.recoveredRuns);
  const activeRunId = useAgentStore((state) => state.activeRunId);
  const agentProfiles = useAgentStore((state) => state.agentProfiles);
  const activeAgentId = useAgentStore((state) => state.activeAgentId);

  const chatProfiles = useModelStore((state) => state.chatProfiles);
  const activeChatProfileId = useModelStore((state) => state.activeChatProfileId);

  const knowledgeBases = useKnowledgeStore((state) => state.knowledgeBases);
  const knowledgeBaseIds = useKnowledgeStore((state) => state.selectedKnowledgeBaseIds);
  const knowledgeFilters = useKnowledgeStore((state) => state.knowledgeFilters);

  const {
    setConversations,
    setSelectedId,
    setMessages,
    setDraft,
    setPendingQuestion,
    setStreamingCitations,
    setAttachments,
    setLoading,
    setLoadingMessages,
    setError,
    setNotice,
    setSmartSearchEnabled,
    setAutoSearchEnabled,
  } = useChatStore.getState();
  const { setAgentRun, setRecoveredRuns, setActiveRunId, setAgentProfiles, setActiveAgentId } = useAgentStore.getState();
  const { setChatProfiles, setActiveChatProfileId } = useModelStore.getState();
  const {
    setKnowledgeBases,
    setSelectedKnowledgeBaseIds: setKnowledgeBaseIds,
    setKnowledgeFilters,
  } = useKnowledgeStore.getState();

  const agentViews = useRef(new Map<string, AgentRunView>());
  const replayingRuns = useRef(new Set<string>());
  const recoveredRunsRef = useRef<RecoveredRun[]>([]);
  const loadedConversationsRef = useRef(false);

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
        : (preferences.restoreSession || loadedConversationsRef.current ? next[0]?.id ?? null : null));
      loadedConversationsRef.current = true;
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
    } finally {
      setLoading(false);
    }
  };

  const loadConversation = async (conversationId: string) => {
    setLoadingMessages(true);
    setStreamingCitations([]);
    try {
      const [nextMessages, nextDraft] = await Promise.all([
        desktop.listMessages(conversationId),
        preferences.saveDrafts ? desktop.getConversationDraft(conversationId) : Promise.resolve(""),
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
    if (!appearanceLoaded) return;
    let mounted = true;
    void Promise.all([
      desktop.listKnowledgeBases(),
      desktop.listProviderProfiles(),
      typeof desktop.listAgentProfiles === "function" ? desktop.listAgentProfiles().catch(() => [] as AgentProfileSummary[]) : Promise.resolve([] as AgentProfileSummary[]),
      desktop.recoverAgentRuns().catch(() => [] as RecoveredRun[]),
    ])
      .then(async ([bases, profiles, agentEntries, recoveries]) => {
        if (!mounted) return;
        const knowledgePreferencesRaw = typeof desktop.getSetting === "function"
          ? await desktop.getSetting("knowledge.preferences").catch(() => null)
          : null;
        const knowledgePreferences = parseKnowledgePreferences(knowledgePreferencesRaw);
        const allKnowledgeBaseIds = bases.map((base) => base.id);
        setKnowledgeBases(bases);
        const configuredKnowledgeBaseIds = knowledgePreferences.defaultKnowledgeBase && allKnowledgeBaseIds.includes(knowledgePreferences.defaultKnowledgeBase)
          ? [knowledgePreferences.defaultKnowledgeBase]
          : allKnowledgeBaseIds;
        setKnowledgeBaseIds(configuredKnowledgeBaseIds);
        setAutoSearchEnabled(knowledgePreferences.autoRetrieve === true);
        const available = profiles.filter((profile) => profile.enabled && profile.model_id && profile.kind !== "mineru");
        setChatProfiles(available);
        setActiveChatProfileId((current) => current && available.some((profile) => profile.id === current)
          ? current
          : available[0]?.id ?? null);
        setAgentProfiles(agentEntries.filter((profile) => profile.enabled));
        recoveredRunsRef.current = recoveries;
        setRecoveredRuns(recoveries);
        await loadConversations();
      })
      .catch((cause) => {
        if (!mounted) return;
        if (errorMessage(cause, "") === "Desktop runtime is unavailable") {
          setConversations([]);
          setSelectedId(null);
          setMessages([]);
          setLoading(false);
          return;
        }
        setError(errorMessage(cause, t("chatError")));
      });
    return () => {
      mounted = false;
    };
  }, [appearanceLoaded, preferences.restoreSession]);

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
      if (event.type === "evidence_attached" && event.data.citation_numbers.length > 0) {
        void Promise.all(event.data.citation_numbers.map((citationNumber) =>
          desktop.resolveKnowledgeCitation(event.data.evidence_pack_id, citationNumber).catch(() => null),
        )).then((citations) => {
          if (!mounted) return;
          const evidence = citations.filter((citation): citation is NonNullable<typeof citation> => citation !== null).map((citation) => ({
            citation_number: citation.citation_number,
            chunk: citation.chunk,
            assets: citation.assets,
          }));
          setStreamingCitations(evidence);
        });
      }
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
    if (!preferences.saveDrafts || !selectedId || loadingMessages || pendingQuestion !== null) return;
    const timer = window.setTimeout(() => {
      void desktop.saveConversationDraft(selectedId, draft).catch((cause) => setError(errorMessage(cause, t("chatError"))));
    }, 450);
    return () => window.clearTimeout(timer);
  }, [draft, loadingMessages, pendingQuestion, preferences.saveDrafts, selectedId]);

  const createConversation = async (initialDraft = "") => {
    setError(null);
    setNotice(null);
    try {
      const created = await desktop.createConversation(t("newConversation"));
      if (preferences.saveDrafts && initialDraft) await desktop.saveConversationDraft(created.id, initialDraft);
      setConversations((current) => [created, ...current]);
      setSelectedId(created.id);
      setMessages([]);
      setDraft(initialDraft);
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
      const shouldSearch = (smartSearchEnabled || autoSearchEnabled) && shouldRunSmartSearch(question);
      setPendingQuestion(submittedMessage);
      setAgentRun(createAgentRunView(runId, conversationId));
      setActiveRunId(runId);
      setDraft("");
      setAttachments([]);
      setStreamingCitations([]);

      let evidencePackId: string | undefined;
      if (shouldSearch && knowledgeBaseIds.length > 0) {
        try {
          const knowledgePreferences = typeof desktop.getSetting === "function"
            ? await desktop.getSetting("knowledge.preferences").then(parseKnowledgePreferences).catch(() => ({ defaultKnowledgeBase: "", citationsEnabled: true, autoRetrieve: false, retrievalOptions: {} }))
            : { defaultKnowledgeBase: "", citationsEnabled: true, autoRetrieve: false, retrievalOptions: {} };
          if (knowledgePreferences.citationsEnabled) {
            const evidencePack = await desktop.queryLocalKnowledge({ query: question, knowledge_base_ids: knowledgeBaseIds, ...knowledgePreferences.retrievalOptions, filters: knowledgeFilters });
            evidencePackId = evidencePack.id;
          }
        } catch (cause) {
          setError(errorMessage(cause, t("chatError")));
        }
      }
      const response = await desktop.desktopAgentChat({
        sessionId: conversationId,
        message: submittedMessage,
        runId,
        agentId: activeAgentId ?? undefined,
        evidencePackId,
        smartSearchEnabled: shouldSearch,
        attachments: submittedAttachments,
      });
      setAgentRun((current) => {
        if (!current || current.runId !== runId || current.assistantText || !response.answer) return current;
        return { ...current, assistantText: response.answer };
      });
      if (preferences.notifications) {
        notifyRunResult("Suna", response.status === "completed" ? t("runtimeReady") : t("chatError"));
      }
      await refreshConversation(conversationId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
      if (preferences.notifications) notifyRunResult("Suna", t("chatError"));
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
        agentId: activeAgentId ?? undefined,
        smartSearchEnabled: smartSearchEnabled || autoSearchEnabled,
      });
      setAgentRun((current) => current?.runId === runId && !current.assistantText && response.answer
        ? { ...current, assistantText: response.answer }
        : current);
      if (preferences.notifications) {
        notifyRunResult("Suna", response.status === "completed" ? t("runtimeReady") : t("chatError"));
      }
      await refreshConversation(selectedId);
    } catch (cause) {
      setError(errorMessage(cause, t("chatError")));
      if (preferences.notifications) notifyRunResult("Suna", t("chatError"));
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

  const selectAgent = (agentId: string | null) => {
    setActiveAgentId(agentId);
  };

  const selectKnowledgeBases = (ids: string[]) => {
    setKnowledgeBaseIds(ids);
  };

  const selectKnowledgeFilters = (filters: PostgresKnowledgeSearchFilters) => {
    setKnowledgeFilters(filters);
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
    agentProfiles,
    activeAgentId,
    knowledgeBases,
    selectedKnowledgeBaseIds: knowledgeBaseIds,
    knowledgeFilters,
    streamingCitations,
    autoKnowledgeSearchEnabled: autoSearchEnabled,
    smartSearchEnabled,
    attachments,
    error,
    notice,
    onNewConversation: (initialDraft) => void createConversation(initialDraft),
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
    onSelectAgent: selectAgent,
    onSelectKnowledgeBases: selectKnowledgeBases,
    onSelectKnowledgeFilters: selectKnowledgeFilters,
    onToggleSmartSearch: () => setSmartSearchEnabled((enabled) => !enabled),
  };
}
