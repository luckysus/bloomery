import { ArrowLeft, Sparkles } from "lucide-react";
import AgentWorkbench from "./AgentWorkbench";
import type { AgentConversation, AgentMessage, AgentPendingConfirmation, AgentResponse } from "./types";

interface AgentModePageProps {
  query: string;
  messages: AgentMessage[];
  response: AgentResponse | null;
  loading: boolean;
  error: string;
  sessionId: string;
  conversations: AgentConversation[];
  onQueryChange: (value: string) => void;
  onSubmit: () => void;
  onClearMemory: () => void;
  onConfirmAction: (item: AgentPendingConfirmation, approved: boolean) => void;
  onNewConversation: () => void;
  onSelectConversation: (conversation: AgentConversation) => void;
  onBack: () => void;
}

export default function AgentModePage({
  query,
  messages,
  response,
  loading,
  error,
  sessionId,
  conversations,
  onQueryChange,
  onSubmit,
  onClearMemory,
  onConfirmAction,
  onNewConversation,
  onSelectConversation,
  onBack,
}: AgentModePageProps) {
  return (
    <div className="relative flex h-screen flex-col overflow-x-hidden overflow-y-auto bg-slate-50 text-slate-900">
      <div className="pointer-events-none fixed inset-0 overflow-hidden">
        <div className="absolute -top-[45%] left-[8%] h-[70vh] w-[50vw] rounded-full bg-cyan-100/60 blur-[130px]" />
        <div className="absolute -bottom-[35%] right-[-10%] h-[65vh] w-[55vw] rounded-full bg-indigo-100/60 blur-[120px]" />
      </div>

      <header className="relative z-10 shrink-0 border-b border-slate-200 bg-white/90 px-6 py-4 shadow-sm backdrop-blur">
        <div className="flex items-center justify-between gap-4">
          <div className="flex min-w-0 items-center gap-4">
            <button
              onClick={onBack}
              className="flex h-10 w-10 items-center justify-center rounded-lg border border-slate-200 text-slate-500 transition-colors hover:bg-slate-50 hover:text-slate-800"
              title="返回模式选择"
            >
              <ArrowLeft size={20} />
            </button>
            <div className="flex h-11 w-11 items-center justify-center rounded-lg bg-gradient-to-br from-indigo-600 to-cyan-500 shadow-lg shadow-indigo-200">
              <Sparkles size={22} className="text-white" />
            </div>
            <div className="min-w-0">
              <h1 className="text-2xl font-bold tracking-normal text-slate-900">钢铁智能体</h1>
              <p className="mt-0.5 truncate text-sm text-slate-500">独立智能体模式：规划、工具调用、证据和推荐方案集中展示</p>
            </div>
          </div>
          <div className="hidden rounded-lg border border-slate-200 bg-slate-50 px-3 py-2 text-sm text-slate-500 md:block">
            会话：{response?.memory?.session_id || sessionId || "未创建"}
          </div>
        </div>
      </header>

      <div className="relative z-10 flex min-h-0 flex-1 pt-4">
        <AgentWorkbench
          messages={messages}
          response={response}
          loading={loading}
          error={error}
          sessionId={sessionId}
          conversations={conversations}
          onClearMemory={onClearMemory}
          onConfirmAction={onConfirmAction}
          onUseQuestion={onQueryChange}
          query={query}
          onQueryChange={onQueryChange}
          onSubmit={onSubmit}
          onNewConversation={onNewConversation}
          onSelectConversation={onSelectConversation}
        />
      </div>
    </div>
  );
}
