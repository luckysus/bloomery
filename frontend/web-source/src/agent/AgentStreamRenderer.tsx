import React from "react";
import { AlertCircle, BotMessageSquare, Settings2, Sparkles } from "lucide-react";
import AgentRecommendationCard from "./AgentRecommendationCard";
import type { AgentResponse } from "./types";

function agentStatusClass(status?: string) {
  if (status === "completed") return "bg-emerald-50 text-emerald-700 border-emerald-100";
  if (status === "needs_confirmation") return "bg-amber-50 text-amber-700 border-amber-100";
  if (status === "needs_input") return "bg-blue-50 text-blue-700 border-blue-100";
  if (status === "failed") return "bg-red-50 text-red-700 border-red-100";
  if (status === "running") return "bg-indigo-50 text-indigo-700 border-indigo-100";
  return "bg-slate-50 text-slate-500 border-slate-200";
}

interface AgentStreamRendererProps {
  response: AgentResponse | null;
  error: string;
  loading: boolean;
  showExecutionDetails: boolean;
  onUseQuestion?: (question: string) => void;
}

const AgentStreamRenderer: React.FC<AgentStreamRendererProps> = ({
  response,
  error,
  loading: _loading,
  showExecutionDetails: _showExecutionDetails,
  onUseQuestion,
}) => {
  return (
    <>
      {error && (
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-base text-red-600 shadow-sm">
          <AlertCircle size={16} className="inline mr-1.5 -mt-0.5" />
          {error}
        </div>
      )}

      {response?.follow_up_questions?.length ? (
        <div className="rounded-xl border border-blue-200 bg-blue-50 p-4 shadow-sm">
          <div className="mb-2 flex items-center gap-2 text-base font-semibold text-blue-800">
            <BotMessageSquare size={16} />
            需要补充的信息
          </div>
          <div className="grid gap-2 md:grid-cols-2">
            {response.follow_up_questions.map((question, idx) => (
              <button
                key={idx}
                onClick={() => onUseQuestion?.(question)}
                className="rounded-lg border border-blue-200 bg-white px-3 py-2 text-left text-base leading-relaxed text-blue-700 transition-colors hover:bg-blue-50"
              >
                {question}
              </button>
            ))}
          </div>
        </div>
      ) : null}

      {response?.answer?.includes("云端检索服务不可用") && (
        <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-base leading-relaxed text-amber-800 shadow-sm">
          云端检索服务不可用：当前已降级为数据库文本检索，专业结论需要服务恢复后复核。
        </div>
      )}

      {response?.intent?.needs_evidence && (response.evidence ?? []).length === 0 && (
        <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3 text-base leading-relaxed text-slate-600 shadow-sm">
          知识库未命中：本轮回答基于模型通用知识，未引用项目文献、标准或生产数据。
        </div>
      )}

      {response?.recommendations?.length ? (
        <div>
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold uppercase tracking-widest text-slate-500">
            <Sparkles size={16} />
            推荐方案
          </div>
          <div className="grid gap-3 xl:grid-cols-2">
            {response.recommendations.map((item, idx) => (
              <AgentRecommendationCard key={`${item.title}-${idx}`} item={item} />
            ))}
          </div>
        </div>
      ) : null}

      {response?.tool_calls?.length ? (
        <div className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
          <div className="mb-3 flex items-center gap-2 text-base font-semibold text-slate-900">
            <Settings2 size={16} className="text-slate-500" />
            工具调用
          </div>
          <div className="space-y-2">
            {response.tool_calls.map((call) => (
              <div key={call.call_id} className="rounded-lg border border-slate-200 p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="font-semibold text-slate-900">{call.title || call.tool_name}</div>
                  <span className={`rounded-full border px-2 py-0.5 text-xs font-semibold ${agentStatusClass(call.status)}`}>
                    {call.status}
                  </span>
                </div>
                <p className="mt-2 text-base leading-relaxed text-slate-600">{call.result_summary || call.status}</p>
                {call.error && <p className="mt-1 text-sm text-red-600">{call.error}</p>}
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </>
  );
};

export { AgentRecommendationCard, agentStatusClass };
export default AgentStreamRenderer;
