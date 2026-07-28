import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import {
  Activity,
  AlertTriangle,
  Archive,
  ArchiveRestore,
  Brain,
  Database,
  Eye,
  FileText,
  GitFork,
  History,
  Plus,
  RefreshCw,
  RotateCcw,
  Scissors,
  Search,
  Send,
  Square,
  X,
} from "lucide-react";
import {
  archiveConversation,
  createConversation,
  forkConversationFromMessage,
  listArchivedConversations,
  listConversations,
  listMessages,
  replaceMessageAfterEdit,
  restoreConversation,
  searchHistory,
  truncateConversationAfterMessage,
  type DesktopConversation,
  type DesktopHistoryHit,
  type DesktopMessage,
} from "./services/conversations";
import {
  buildContextPacket,
  type DesktopContextHistoryHit,
  type DesktopContextMemory,
  type DesktopContextMessage,
  type DesktopContextPacket,
} from "./services/contextPacket";
import { confirmDesktopCloudJob, streamDesktopAgent } from "./services/localAgent";
import { saveMemory } from "./services/memories";
import { clearConversationDraft, getConversationDraft, saveConversationDraft } from "./services/drafts";
import { summarizeConversation } from "./services/summaries";
import type { AgentPendingConfirmation } from "../agent/types";

const NEW_DRAFT_KEY = "__new__";

function truncateText(value = "", limit = 180) {
  const chars = Array.from(value);
  return chars.length > limit ? `${chars.slice(0, limit).join("")}...` : value;
}

export default function DesktopChatPage() {
  const [conversations, setConversations] = useState<DesktopConversation[]>([]);
  const [archivedConversations, setArchivedConversations] = useState<DesktopConversation[]>([]);
  const [showArchived, setShowArchived] = useState(false);
  const [activeId, setActiveId] = useState("");
  const [messages, setMessages] = useState<DesktopMessage[]>([]);
  const [input, setInput] = useState("");
  const [assistantDraft, setAssistantDraft] = useState("");
  const [lastContextPacket, setLastContextPacket] = useState<DesktopContextPacket | null>(null);
  const [contextOpen, setContextOpen] = useState(false);
  const [pendingConfirmation, setPendingConfirmation] = useState<AgentPendingConfirmation | null>(null);
  const [confirmingActionId, setConfirmingActionId] = useState("");
  const [historyQuery, setHistoryQuery] = useState("");
  const [historyResults, setHistoryResults] = useState<DesktopHistoryHit[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [loading, setLoading] = useState(false);
  const [summarizingId, setSummarizingId] = useState("");
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const abortRef = useRef<AbortController | null>(null);

  const activeConversation = useMemo(
    () => conversations.find((item) => item.id === activeId) || null,
    [activeId, conversations],
  );
  const draftKey = activeId || NEW_DRAFT_KEY;
  const budget = lastContextPacket?.desktop_meta?.budget_meta;
  const visiblePendingConfirmation =
    pendingConfirmation && readConfirmationArg(pendingConfirmation, "conversation_id") === activeId
      ? pendingConfirmation
      : null;

  const refreshConversations = async () => {
    const items = await listConversations();
    setConversations(items);
    if (!activeId && items[0]) setActiveId(items[0].id);
  };

  const refreshArchivedConversations = async () => {
    setArchivedConversations(await listArchivedConversations());
  };

  useEffect(() => {
    void refreshConversations().catch((err) => setError(String(err)));
  }, []);

  useEffect(() => {
    if (showArchived) void refreshArchivedConversations().catch((err) => setError(String(err)));
  }, [showArchived]);

  useEffect(() => {
    let cancelled = false;
    void getConversationDraft(draftKey)
      .then((value) => {
        if (!cancelled) setInput(value);
      })
      .catch((err) => setError(String(err)));
    return () => {
      cancelled = true;
    };
  }, [draftKey]);

  useEffect(() => {
    if (!activeId) {
      setMessages([]);
      return;
    }
    void listMessages(activeId).then(setMessages).catch((err) => setError(String(err)));
  }, [activeId]);

  const ensureConversation = async (text: string) => {
    if (activeConversation) return activeConversation;
    const title = text.trim().slice(0, 28) || "新对话";
    const conversation = await createConversation(title);
    setConversations((items) => [conversation, ...items]);
    setActiveId(conversation.id);
    return conversation;
  };

  const handleInputChange = (value: string) => {
    setInput(value);
    void saveConversationDraft(draftKey, value).catch((err) => setError(String(err)));
  };

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const text = input.trim();
    if (!text || loading) return;
    const submittedDraftKey = draftKey;
    setInput("");
    void clearConversationDraft(submittedDraftKey).catch((err) => setError(String(err)));
    setError("");
    setNotice("");
    setAssistantDraft("");
    setPendingConfirmation(null);
    setLoading(true);

    const controller = new AbortController();
    abortRef.current = controller;
    let conversation: DesktopConversation | null = null;
    let assistantText = "";

    try {
      conversation = await ensureConversation(text);
      const userMessage: DesktopMessage = {
        id: `pending-${Date.now()}`,
        conversation_id: conversation.id,
        role: "user",
        content: text,
        response_json: null,
        created_at: new Date().toISOString(),
      };
      setMessages((items) => [...items, userMessage]);
      const desktopContext = await buildContextPacket(conversation.id, text);
      setLastContextPacket(desktopContext);
      const response = await streamDesktopAgent(
        {
          sessionId: conversation.id,
          message: text,
        },
        (delta) => {
          assistantText += delta;
          setAssistantDraft(assistantText);
        },
        controller.signal,
      );
      setPendingConfirmation(response.pending_confirmations?.[0] ?? null);
      const nextMessages = await listMessages(conversation.id);
      setMessages(nextMessages);
      setAssistantDraft("");
      await maybeSummarize(conversation.id, controller.signal);
      await refreshConversations();
    } catch (err) {
      if (controller.signal.aborted) {
        if (conversation) {
          const latest = await listMessages(conversation.id).catch(() => []);
          if (latest.length) setMessages(latest);
        }
        setNotice("已停止接收本地生成流。");
      } else {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        setInput(text);
        void saveConversationDraft(submittedDraftKey, text).catch((draftErr) => setError(String(draftErr)));
      }
    } finally {
      setLoading(false);
      abortRef.current = null;
      setAssistantDraft("");
    }
  };

  const maybeSummarize = async (conversationId: string, signal: AbortSignal) => {
    await summarizeConversation(conversationId, undefined, signal).catch(() => {});
  };

  const handleStop = () => {
    abortRef.current?.abort();
  };

  const handleConfirmCloudTask = async (item: AgentPendingConfirmation, approved: boolean) => {
    const conversationId = readConfirmationArg(item, "conversation_id") || activeId;
    const taskType = readConfirmationArg(item, "task_type") || item.tool_name;
    const message = readConfirmationArg(item, "user_message") || "";
    if (!conversationId || !taskType) {
      setError("Cloud task confirmation is missing required desktop metadata.");
      return;
    }
    setConfirmingActionId(item.action_id);
    setError("");
    try {
      const response = await confirmDesktopCloudJob({
        conversationId,
        actionId: item.action_id,
        taskType,
        message,
        approved,
      });
      setPendingConfirmation(null);
      setNotice(response.answer || (approved ? "Cloud task confirmation handled." : "Cloud task cancelled."));
      if (conversationId === activeId) {
        setMessages(await listMessages(conversationId));
      }
      await refreshConversations();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setConfirmingActionId("");
    }
  };

  const handleArchive = async (conversationId: string) => {
    await archiveConversation(conversationId);
    if (conversationId === activeId) {
      setActiveId("");
      setMessages([]);
      setLastContextPacket(null);
      setPendingConfirmation(null);
    }
    await refreshConversations();
    if (showArchived) await refreshArchivedConversations();
  };

  const handleRestore = async (conversationId: string) => {
    await restoreConversation(conversationId);
    await refreshConversations();
    await refreshArchivedConversations();
    setShowArchived(false);
    setActiveId(conversationId);
  };

  const runHistorySearch = async (event?: FormEvent) => {
    event?.preventDefault();
    const query = historyQuery.trim();
    if (!query) {
      setHistoryResults([]);
      return;
    }
    setHistoryLoading(true);
    setError("");
    try {
      setHistoryResults(await searchHistory(query, activeId || undefined, false, 10));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setHistoryLoading(false);
    }
  };

  const handleSelectHistoryHit = (hit: DesktopHistoryHit) => {
    setActiveId(hit.conversation_id);
    setPendingConfirmation(null);
    setHistoryQuery("");
    setHistoryResults([]);
  };

  const handleFork = async (messageId: string) => {
    const conversation = await forkConversationFromMessage(messageId);
    setConversations((items) => [conversation, ...items]);
    setActiveId(conversation.id);
    setNotice("已从该消息分叉出新对话。");
  };

  const handleTruncate = async (messageId: string) => {
    await truncateConversationAfterMessage(messageId);
    if (activeId) setMessages(await listMessages(activeId));
    setLastContextPacket(null);
    setNotice("已删除该消息之后的本地消息，并清空旧摘要。");
  };

  const handleRewriteFromUserMessage = async (message: DesktopMessage) => {
    await replaceMessageAfterEdit(message.id, message.content);
    if (activeId) setMessages(await listMessages(activeId));
    handleInputChange(message.content);
    setNotice("已回退到这条用户消息，可修改后重新发送。");
  };

  const handleSaveMessageAsMemory = async (message: DesktopMessage) => {
    await saveMemory({
      scope: "domain",
      type: message.role === "user" ? "user" : "feedback",
      title: `对话记忆候选 - ${truncateText(message.content, 32)}`,
      description: "从本地对话手动沉淀，默认停用，确认后可在记忆页启用。",
      body: message.content,
      tags_json: JSON.stringify(["chat-candidate"]),
      enabled: false,
    });
    setNotice("已保存为停用状态的记忆候选，可到记忆页编辑启用。");
  };

  const handleSummarizeToMessage = async (message: DesktopMessage) => {
    if (!activeId) return;
    setSummarizingId(message.id);
    setError("");
    try {
      const result = await summarizeConversation(activeId, message.id);
      if (result.summarized) {
        setNotice("已把该消息之前的对话保存为本地摘要。");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSummarizingId("");
    }
  };

  const visibleConversations = showArchived ? archivedConversations : conversations;

  return (
    <div className="flex min-h-0 flex-1">
      <aside className="w-80 shrink-0 border-r border-slate-800 bg-slate-950">
        <div className="border-b border-slate-800 p-3">
          <div className="mb-3 flex items-center justify-between">
            <span className="text-sm font-semibold text-slate-200">{showArchived ? "已归档对话" : "本地对话"}</span>
            <div className="flex gap-2">
              <button
                type="button"
                title={showArchived ? "返回本地对话" : "查看归档"}
                onClick={() => setShowArchived((value) => !value)}
                className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-slate-700 text-slate-300 hover:bg-slate-900"
              >
                {showArchived ? <ArchiveRestore className="h-4 w-4" /> : <Archive className="h-4 w-4" />}
              </button>
              <button
                type="button"
                title="新建对话"
                onClick={() => {
                  setActiveId("");
                  setMessages([]);
                  setLastContextPacket(null);
                  setPendingConfirmation(null);
                }}
                className="inline-flex h-8 w-8 items-center justify-center rounded-md bg-cyan-500 text-slate-950 hover:bg-cyan-400"
              >
                <Plus className="h-4 w-4" />
              </button>
            </div>
          </div>
          {!showArchived && (
            <form onSubmit={runHistorySearch} className="space-y-2">
              <div className="flex gap-2">
                <input
                  value={historyQuery}
                  onChange={(event) => setHistoryQuery(event.target.value)}
                  placeholder="搜索本地历史"
                  className="min-w-0 flex-1 rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
                />
                <button
                  type="submit"
                  title="搜索历史"
                  className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-slate-800 text-slate-100 hover:bg-slate-700"
                >
                  {historyLoading ? <RefreshCw className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
                </button>
              </div>
              {historyResults.length > 0 && (
                <div className="max-h-52 space-y-1 overflow-auto rounded-md border border-slate-800 bg-slate-900 p-2">
                  {historyResults.map((hit) => (
                    <button
                      key={hit.message_id}
                      type="button"
                      onClick={() => handleSelectHistoryHit(hit)}
                      className="block w-full rounded-md px-2 py-1.5 text-left text-xs text-slate-300 hover:bg-slate-800"
                    >
                      <span className="block truncate text-slate-100">{hit.conversation_title}</span>
                      <span className="line-clamp-2 text-slate-500">{hit.snippet || truncateText(hit.content, 90)}</span>
                    </button>
                  ))}
                </div>
              )}
            </form>
          )}
        </div>
        <div className="h-[calc(100vh-145px)] overflow-auto p-2">
          {visibleConversations.map((conversation) => (
            <div
              key={conversation.id}
              className={`mb-1 rounded-md border px-3 py-2 text-left text-sm ${
                conversation.id === activeId
                  ? "border-cyan-500 bg-cyan-500/10 text-cyan-100"
                  : "border-slate-800 text-slate-300 hover:bg-slate-900"
              }`}
            >
              <button type="button" className="block w-full text-left" onClick={() => setActiveId(conversation.id)}>
                <span className="block truncate">{conversation.title}</span>
                <span className="text-xs text-slate-500">{new Date(conversation.updated_at).toLocaleString()}</span>
              </button>
              <div className="mt-2 flex gap-2">
                {showArchived ? (
                  <button
                    type="button"
                    className="inline-flex items-center gap-1 text-xs text-cyan-200 hover:text-cyan-100"
                    onClick={() => void handleRestore(conversation.id).catch((err) => setError(String(err)))}
                  >
                    <ArchiveRestore className="h-3.5 w-3.5" />
                    恢复
                  </button>
                ) : (
                  <button
                    type="button"
                    className="inline-flex items-center gap-1 text-xs text-slate-500 hover:text-red-300"
                    onClick={() => void handleArchive(conversation.id).catch((err) => setError(String(err)))}
                  >
                    <Archive className="h-3.5 w-3.5" />
                    归档
                  </button>
                )}
              </div>
            </div>
          ))}
          {visibleConversations.length === 0 && (
            <p className="p-3 text-sm text-slate-500">{showArchived ? "暂无已归档对话。" : "还没有本地对话。"}</p>
          )}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col bg-slate-950">
        <div className="border-b border-slate-800 px-5 py-3">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 className="text-lg font-semibold text-slate-100">{activeConversation?.title || "新对话"}</h2>
              <p className="text-xs text-slate-500">消息保存在本机 SQLite，云端只负责智能体回答。</p>
            </div>
            <button
              type="button"
              onClick={() => setContextOpen((value) => !value)}
              className="inline-flex items-center gap-2 rounded-md border border-slate-700 px-3 py-2 text-xs text-slate-200 hover:bg-slate-900"
            >
              <Eye className="h-4 w-4" />
              {contextOpen ? "收起上下文" : "查看上下文"}
            </button>
          </div>
          <ContextGauge packet={lastContextPacket} />
        </div>

        {contextOpen && <ContextInspector packet={lastContextPacket} />}

        {visiblePendingConfirmation && (
          <CloudTaskConfirmation
            item={visiblePendingConfirmation}
            busy={confirmingActionId === visiblePendingConfirmation.action_id}
            onConfirm={handleConfirmCloudTask}
          />
        )}

        <div className="min-h-0 flex-1 space-y-3 overflow-auto p-5">
          {messages.map((message, index) => (
            <article
              key={message.id}
              className={`max-w-3xl rounded-md border p-3 text-sm leading-6 ${
                message.role === "user"
                  ? "ml-auto border-cyan-700 bg-cyan-950/30 text-cyan-50"
                  : "border-slate-800 bg-slate-900 text-slate-200"
              }`}
            >
              <div className="mb-2 flex items-center justify-between gap-3">
                <div className="text-xs uppercase text-slate-500">{message.role}</div>
                <div className="flex flex-wrap justify-end gap-1">
                  {message.role === "user" && (
                    <>
                      <IconAction title="回退并重问" onClick={() => void handleRewriteFromUserMessage(message).catch((err) => setError(String(err)))}>
                        <RotateCcw className="h-3.5 w-3.5" />
                      </IconAction>
                      <IconAction title="填入输入框" onClick={() => handleInputChange(message.content)}>
                        <Send className="h-3.5 w-3.5" />
                      </IconAction>
                    </>
                  )}
                  <IconAction title="从这里分叉" onClick={() => void handleFork(message.id).catch((err) => setError(String(err)))}>
                    <GitFork className="h-3.5 w-3.5" />
                  </IconAction>
                  <IconAction title="删除后续消息" onClick={() => void handleTruncate(message.id).catch((err) => setError(String(err)))}>
                    <Scissors className="h-3.5 w-3.5" />
                  </IconAction>
                  <IconAction title="总结到这里" onClick={() => void handleSummarizeToMessage(message)}>
                    {summarizingId === message.id ? <RefreshCw className="h-3.5 w-3.5 animate-spin" /> : <FileText className="h-3.5 w-3.5" />}
                  </IconAction>
                  <IconAction title="保存为记忆候选" onClick={() => void handleSaveMessageAsMemory(message).catch((err) => setError(String(err)))}>
                    <Brain className="h-3.5 w-3.5" />
                  </IconAction>
                </div>
              </div>
              <div className="whitespace-pre-wrap">{message.content}</div>
            </article>
          ))}
          {assistantDraft && (
            <article className="max-w-3xl rounded-md border border-slate-800 bg-slate-900 p-3 text-sm leading-6 text-slate-200">
              <div className="mb-1 text-xs uppercase text-slate-500">assistant</div>
              <div className="whitespace-pre-wrap">{assistantDraft}</div>
            </article>
          )}
          {!messages.length && !assistantDraft && (
            <div className="flex h-full items-center justify-center text-sm text-slate-500">
              输入问题开始桌面端本地对话。
            </div>
          )}
        </div>

        {notice && (
          <div className="border-t border-cyan-900 bg-cyan-950/30 px-5 py-2 text-sm text-cyan-100">
            {notice}
          </div>
        )}
        {error && <div className="border-t border-red-900 bg-red-950/40 px-5 py-2 text-sm text-red-200">{error}</div>}

        <form onSubmit={handleSubmit} className="border-t border-slate-800 p-4">
          <div className="flex gap-3">
            <textarea
              value={input}
              onChange={(event) => handleInputChange(event.target.value)}
              placeholder="向钢铁智能体提问..."
              rows={2}
              className="min-h-[52px] flex-1 resize-none rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
            />
            {loading ? (
              <button
                type="button"
                onClick={handleStop}
                className="inline-flex h-[52px] items-center gap-2 rounded-md border border-red-800 px-5 text-sm font-semibold text-red-100 hover:bg-red-950"
              >
                <Square className="h-4 w-4" />
                停止
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim()}
                className="inline-flex h-[52px] items-center gap-2 rounded-md bg-cyan-500 px-5 text-sm font-semibold text-slate-950 hover:bg-cyan-400 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Send className="h-4 w-4" />
                发送
              </button>
            )}
          </div>
        </form>
      </section>
    </div>
  );
}

function readConfirmationArg(item: AgentPendingConfirmation, key: string) {
  const value = item.arguments?.[key];
  if (typeof value === "string") return value.trim();
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return "";
}

function CloudTaskConfirmation({
  item,
  busy,
  onConfirm,
}: {
  item: AgentPendingConfirmation;
  busy: boolean;
  onConfirm: (item: AgentPendingConfirmation, approved: boolean) => void | Promise<void>;
}) {
  const taskType = readConfirmationArg(item, "task_type") || item.tool_name;
  const estimatedTime = readConfirmationArg(item, "estimated_time");
  const resourceUsage = readConfirmationArg(item, "resource_usage");
  const isDanger = item.permission === "danger";
  return (
    <div className={`border-b px-5 py-3 ${isDanger ? "border-red-900 bg-red-950/30" : "border-amber-900 bg-amber-950/20"}`}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <div className={`flex items-center gap-2 text-sm font-semibold ${isDanger ? "text-red-100" : "text-amber-100"}`}>
            <AlertTriangle className="h-4 w-4" />
            <span>{item.title || "Cloud task confirmation"}</span>
            <span className="rounded-md border border-slate-700 px-1.5 py-0.5 text-[11px] uppercase text-slate-400">
              {taskType}
            </span>
          </div>
          <p className="mt-1 text-xs text-slate-400">{item.warning || "Confirm before sending this task to the cloud backend."}</p>
          {(estimatedTime || resourceUsage) && (
            <p className="mt-1 text-xs text-slate-500">
              {[estimatedTime && `time: ${estimatedTime}`, resourceUsage && `resource: ${resourceUsage}`].filter(Boolean).join(" / ")}
            </p>
          )}
        </div>
        <div className="flex shrink-0 gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void onConfirm(item, false)}
            className="inline-flex h-9 items-center rounded-md border border-slate-700 px-3 text-xs text-slate-300 hover:bg-slate-900 disabled:cursor-not-allowed disabled:opacity-60"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void onConfirm(item, true)}
            className={`inline-flex h-9 items-center gap-2 rounded-md px-3 text-xs font-semibold disabled:cursor-not-allowed disabled:opacity-60 ${
              isDanger ? "bg-red-500 text-white hover:bg-red-400" : "bg-amber-400 text-slate-950 hover:bg-amber-300"
            }`}
          >
            {busy && <RefreshCw className="h-3.5 w-3.5 animate-spin" />}
            Confirm
          </button>
        </div>
      </div>
    </div>
  );
}

function IconAction({ title, onClick, children }: { title: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-slate-700 text-slate-400 hover:border-cyan-800 hover:bg-slate-800 hover:text-cyan-100"
    >
      {children}
    </button>
  );
}

function ContextGauge({ packet }: { packet: DesktopContextPacket | null }) {
  const budget = packet?.desktop_meta?.budget_meta;
  const stats = [
    { label: "recent", value: budget?.recent_message_count ?? 0, icon: History },
    { label: "记忆命中", value: budget?.selected_memory_count ?? 0, icon: Brain },
    { label: "历史命中", value: budget?.history_hit_count ?? 0, icon: Database },
    { label: "摘要", value: (budget?.summary_tokens ?? 0) > 0 ? "有" : "无", icon: FileText },
    { label: "估算 tokens", value: budget?.estimated_context_tokens ?? 0, icon: Activity },
  ];
  return (
    <div className="mt-3 flex flex-wrap gap-2">
      {stats.map((item) => {
        const Icon = item.icon;
        return (
          <span key={item.label} className="inline-flex items-center gap-1.5 rounded-md border border-slate-800 bg-slate-900 px-2.5 py-1 text-xs text-slate-300">
            <Icon className="h-3.5 w-3.5 text-cyan-300" />
            {item.label}: <strong className="font-semibold text-slate-100">{item.value}</strong>
          </span>
        );
      })}
      <span className="inline-flex items-center rounded-md border border-slate-800 bg-slate-900 px-2.5 py-1 text-xs text-slate-500">
        v{packet?.desktop_meta?.context_version ?? 2}
      </span>
    </div>
  );
}

function ContextInspector({ packet }: { packet: DesktopContextPacket | null }) {
  if (!packet) {
    return (
      <div className="border-b border-slate-800 bg-slate-950 px-5 py-4 text-sm text-slate-500">
        发送一条消息后，这里会显示本轮注入云端的本地上下文。
      </div>
    );
  }
  return (
    <div className="max-h-80 overflow-auto border-b border-slate-800 bg-slate-950 px-5 py-4">
      <div className="grid gap-3 xl:grid-cols-2">
        <InspectorSection title="会话摘要">
          <p className="whitespace-pre-wrap text-sm text-slate-300">
            {packet.conversation_summary ? truncateText(packet.conversation_summary, 700) : "本轮没有注入摘要。"}
          </p>
        </InspectorSection>
        <InspectorSection title="长期记忆命中">
          <MemoryList items={packet.selected_memories} showBody />
        </InspectorSection>
        <InspectorSection title="历史对话命中">
          <HistoryHitList items={packet.history_hits} />
        </InspectorSection>
        <InspectorSection title="recent messages">
          <RecentMessageList items={packet.recent_messages} />
        </InspectorSection>
        <InspectorSection title="记忆索引">
          <MemoryList items={packet.memory_index} />
        </InspectorSection>
      </div>
    </div>
  );
}

function InspectorSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-md border border-slate-800 bg-slate-900 p-3">
      <h3 className="mb-2 text-xs font-semibold uppercase text-slate-500">{title}</h3>
      {children}
    </section>
  );
}

function MemoryList({ items, showBody = false }: { items: DesktopContextMemory[]; showBody?: boolean }) {
  if (!items.length) return <p className="text-sm text-slate-500">无</p>;
  return (
    <div className="space-y-2">
      {items.slice(0, 8).map((item, index) => (
        <div key={item.id || index} className="text-sm text-slate-300">
          <div className="font-medium text-slate-100">{item.title || item.id || `记忆 ${index + 1}`}</div>
          <div className="text-xs text-slate-500">
            {[item.scope, item.type, formatTags(item.tags_json)].filter(Boolean).join(" / ")}
          </div>
          {item.snippet && <p className="mt-1 text-xs text-cyan-200">{truncateText(item.snippet, 180)}</p>}
          {showBody && item.body && <p className="mt-1 line-clamp-3 text-xs text-slate-400">{item.body}</p>}
        </div>
      ))}
    </div>
  );
}

function HistoryHitList({ items }: { items: DesktopContextHistoryHit[] }) {
  if (!items.length) return <p className="text-sm text-slate-500">无</p>;
  return (
    <div className="space-y-2">
      {items.map((item, index) => (
        <div key={item.message_id || index} className="text-sm text-slate-300">
          <div className="font-medium text-slate-100">{item.conversation_title || `历史 ${index + 1}`}</div>
          <div className="text-xs text-slate-500">{item.role} / {item.created_at}</div>
          <p className="mt-1 line-clamp-3 text-xs text-slate-400">{item.snippet || item.content}</p>
        </div>
      ))}
    </div>
  );
}

function RecentMessageList({ items }: { items: DesktopContextMessage[] }) {
  if (!items.length) return <p className="text-sm text-slate-500">无</p>;
  return (
    <div className="space-y-2">
      {items.slice(-8).map((item, index) => (
        <div key={`${item.created_at || ""}-${index}`} className="text-sm text-slate-300">
          <div className="text-xs uppercase text-slate-500">{item.role || "message"}</div>
          <p className="line-clamp-2 text-xs text-slate-400">{truncateText(item.content, 180)}</p>
        </div>
      ))}
    </div>
  );
}

function formatTags(tagsJson?: string) {
  if (!tagsJson) return "";
  try {
    const tags = JSON.parse(tagsJson);
    return Array.isArray(tags) ? tags.join(", ") : "";
  } catch {
    return "";
  }
}
