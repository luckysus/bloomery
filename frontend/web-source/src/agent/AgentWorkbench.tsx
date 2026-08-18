import React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { AlertCircle, CheckCircle2, MessageSquarePlus, Send, XCircle } from "lucide-react";
import type {
  AgentConversation,
  AgentMessage,
  AgentPendingConfirmation,
  AgentResponse,
} from "./types";

interface AgentWorkbenchProps {
  messages: AgentMessage[];
  response: AgentResponse | null;
  loading: boolean;
  error: string;
  sessionId: string;
  conversations?: AgentConversation[];
  onClearMemory: () => void;
  onConfirmAction: (item: AgentPendingConfirmation, approved: boolean) => void;
  onUseQuestion: (question: string) => void;
  query?: string;
  onQueryChange?: (value: string) => void;
  onSubmit?: () => void;
  onNewConversation?: () => void;
  onSelectConversation?: (conversation: AgentConversation) => void;
  showHistory?: boolean;
}

function formatTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export default function AgentWorkbench({
  messages,
  response,
  loading,
  error,
  sessionId,
  conversations = [],
  onClearMemory,
  onConfirmAction,
  onUseQuestion,
  query,
  onQueryChange,
  onSubmit,
  onNewConversation,
  onSelectConversation,
  showHistory = true,
}: AgentWorkbenchProps) {
  const hasComposer = query !== undefined && onQueryChange && onSubmit;
  const followUps = response?.follow_up_questions ?? [];
  const confirmations = response?.pending_confirmations ?? [];
  const evidence = response?.evidence ?? [];
  const recommendations = response?.recommendations ?? [];

  return (
    <section className="flex h-full min-h-0 w-full flex-1 overflow-hidden px-6 pb-4">
      <div
        className={`grid min-h-0 w-full gap-3 ${
          showHistory
            ? "xl:grid-cols-[240px_minmax(480px,1fr)_minmax(360px,0.65fr)]"
            : "xl:grid-cols-[minmax(560px,1fr)_minmax(360px,0.65fr)]"
        }`}
      >
        {showHistory && (
          <aside className="min-h-0 rounded-xl border border-slate-200 bg-white shadow-sm">
            <div className="flex items-center justify-between border-b border-slate-100 px-3 py-3">
              <div>
                <h3 className="text-base font-bold text-slate-900">对话历史</h3>
                <div className="mt-0.5 text-xs text-slate-400">{conversations.length} 个会话</div>
              </div>
              {onNewConversation && (
                <button
                  onClick={onNewConversation}
                  className="flex h-8 w-8 items-center justify-center rounded-md border border-slate-200 text-slate-500 transition hover:bg-slate-50 hover:text-indigo-600"
                  title="新建对话"
                >
                  <MessageSquarePlus size={16} />
                </button>
              )}
            </div>
            <div className="max-h-full overflow-auto p-2">
              {conversations.length === 0 ? (
                <div className="rounded-lg border border-dashed border-slate-200 bg-slate-50 px-3 py-4 text-sm text-slate-500">
                  暂无对话历史
                </div>
              ) : (
                conversations.map((conversation) => (
                  <button
                    key={conversation.sessionId}
                    onClick={() => onSelectConversation?.(conversation)}
                    className={`mb-1 w-full rounded-lg px-3 py-2 text-left text-sm transition ${
                      conversation.sessionId === sessionId
                        ? "bg-indigo-50 text-indigo-700"
                        : "text-slate-600 hover:bg-slate-50"
                    }`}
                  >
                    <div className="truncate font-semibold">{conversation.title}</div>
                    <div className="mt-0.5 text-xs text-slate-400">{formatTime(conversation.updatedAt)}</div>
                  </button>
                ))
              )}
            </div>
          </aside>
        )}

        <main className="flex min-h-0 flex-col rounded-xl border border-slate-200 bg-white shadow-sm">
          <div className="flex items-center justify-between border-b border-slate-100 px-4 py-3">
            <div>
              <h2 className="text-lg font-bold text-slate-900">智能体对话</h2>
              <p className="mt-0.5 text-sm text-slate-400">规划、检索、证据和工具调用会汇总在这里。</p>
            </div>
            <button
              onClick={onClearMemory}
              className="rounded-lg border border-slate-200 px-3 py-1.5 text-sm font-semibold text-slate-500 transition hover:bg-slate-50 hover:text-slate-700"
            >
              清除记忆
            </button>
          </div>

          <div className="min-h-0 flex-1 overflow-auto px-4 py-4">
            {messages.length === 0 ? (
              <div className="flex h-full min-h-[260px] items-center justify-center text-center text-slate-400">
                输入目标或问题后开始对话。
              </div>
            ) : (
              <div className="space-y-4">
                {messages.map((message, index) => (
                  <div key={index} className={message.role === "user" ? "flex justify-end" : "block"}>
                    {message.role === "user" ? (
                      <div className="max-w-[70%] rounded-2xl bg-slate-900 px-4 py-3 text-base leading-relaxed text-white">
                        {message.content}
                      </div>
                    ) : (
                      <div className="ai-markdown-body text-[17px] leading-relaxed text-slate-700">
                        <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
                      </div>
                    )}
                  </div>
                ))}
                {loading && (
                  <div className="flex h-7 items-center gap-1.5">
                    <span className="h-2 w-2 animate-bounce rounded-full bg-slate-400 [animation-delay:-0.24s]" />
                    <span className="h-2 w-2 animate-bounce rounded-full bg-slate-400 [animation-delay:-0.12s]" />
                    <span className="h-2 w-2 animate-bounce rounded-full bg-slate-400" />
                  </div>
                )}
                {error && (
                  <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-600">
                    <AlertCircle size={15} className="mr-1 inline" />
                    {error}
                  </div>
                )}
              </div>
            )}
          </div>

          {hasComposer && (
            <div className="border-t border-slate-100 p-3">
              <div className="flex items-end gap-2 rounded-2xl border border-slate-200 bg-slate-50 p-2">
                <textarea
                  value={query}
                  onChange={(event) => onQueryChange(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && !event.shiftKey) {
                      event.preventDefault();
                      onSubmit();
                    }
                  }}
                  rows={2}
                  className="min-h-[44px] flex-1 resize-none bg-transparent px-2 py-2 text-base outline-none"
                  placeholder="输入问题或目标..."
                />
                <button
                  onClick={onSubmit}
                  disabled={loading || !query?.trim()}
                  className="flex h-10 w-10 items-center justify-center rounded-xl bg-indigo-600 text-white transition hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-40"
                  title="发送"
                >
                  <Send size={18} />
                </button>
              </div>
            </div>
          )}
        </main>

        <aside className="min-h-0 overflow-auto rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
          <div className="flex items-center justify-between">
            <h3 className="text-base font-bold text-slate-900">运行信息</h3>
          </div>

          {followUps.length > 0 && (
            <div className="mt-4">
              <div className="mb-2 text-sm font-semibold text-slate-500">需要补充的信息</div>
              <div className="space-y-2">
                {followUps.map((question, index) => (
                  <button
                    key={index}
                    onClick={() => onUseQuestion(question)}
                    className="w-full rounded-lg border border-blue-100 bg-blue-50 px-3 py-2 text-left text-sm text-blue-700 transition hover:bg-blue-100"
                  >
                    {question}
                  </button>
                ))}
              </div>
            </div>
          )}

          {confirmations.length > 0 && (
            <div className="mt-4 space-y-2">
              <div className="text-sm font-semibold text-slate-500">待确认操作</div>
              {confirmations.map((item) => (
                <div key={item.action_id} className="rounded-lg border border-amber-200 bg-amber-50 p-3">
                  <div className="font-semibold text-amber-800">{item.title}</div>
                  {item.warning && <div className="mt-1 text-sm text-amber-700">{item.warning}</div>}
                  <div className="mt-3 flex gap-2">
                    <button
                      onClick={() => onConfirmAction(item, true)}
                      className="flex items-center gap-1 rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-semibold text-white"
                    >
                      <CheckCircle2 size={14} />
                      确认
                    </button>
                    <button
                      onClick={() => onConfirmAction(item, false)}
                      className="flex items-center gap-1 rounded-md bg-white px-3 py-1.5 text-sm font-semibold text-slate-600 ring-1 ring-slate-200"
                    >
                      <XCircle size={14} />
                      取消
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {recommendations.length > 0 && (
            <div className="mt-4">
              <div className="mb-2 text-sm font-semibold text-slate-500">推荐方案</div>
              <div className="space-y-2">
                {recommendations.map((item, index) => (
                  <div key={`${item.title}-${index}`} className="rounded-lg border border-slate-200 p-3">
                    <div className="font-semibold text-slate-900">{item.title}</div>
                    {item.summary && <div className="mt-1 text-sm leading-relaxed text-slate-600">{item.summary}</div>}
                  </div>
                ))}
              </div>
            </div>
          )}

          {evidence.length > 0 && (
            <div className="mt-4">
              <div className="mb-2 text-sm font-semibold text-slate-500">证据</div>
              <div className="space-y-2">
                {evidence.slice(0, 6).map((item) => (
                  <div key={item.evidence_id} className="rounded-lg bg-slate-50 px-3 py-2 text-sm text-slate-600">
                    <div className="font-semibold text-slate-800">{item.title || item.source_label}</div>
                    {item.evidence_level && <div className="mt-0.5 text-xs text-slate-400">{item.evidence_level}</div>}
                  </div>
                ))}
              </div>
            </div>
          )}

        </aside>
      </div>
    </section>
  );
}
