import { Check } from "lucide-react";

export type AgentProgressStepKey = "analysis" | "intent" | "retrieval" | "organize" | "answer";

export interface AgentProgressState {
  active: boolean;
  current: AgentProgressStepKey;
  completed: AgentProgressStepKey[];
  statusText: string;
  mode: "direct" | "workflow";
}

const AGENT_PROGRESS_STEPS: Array<{ key: AgentProgressStepKey; label: string }> = [
  { key: "analysis", label: "问题分析" },
  { key: "intent", label: "意图理解" },
  { key: "retrieval", label: "检索/工具" },
  { key: "organize", label: "内容整理" },
  { key: "answer", label: "完成回答" },
];

export const AGENT_PROGRESS_ORDER = AGENT_PROGRESS_STEPS.map((step) => step.key);

export const initialAgentProgress: AgentProgressState = {
  active: false,
  current: "analysis",
  completed: [],
  statusText: "准备分析问题...",
  mode: "workflow",
};

function progressToIndex(key: AgentProgressStepKey) {
  return AGENT_PROGRESS_ORDER.indexOf(key);
}

export function buildAgentProgress(
  current: AgentProgressStepKey,
  statusText: string,
  mode: "direct" | "workflow" = "workflow",
): AgentProgressState {
  const currentIndex = progressToIndex(current);
  return {
    active: true,
    current,
    completed: AGENT_PROGRESS_ORDER.slice(0, Math.max(0, currentIndex)),
    statusText,
    mode,
  };
}

export function progressFromWorkflowNode(nodeType?: string): { step: AgentProgressStepKey; text: string } {
  const type = (nodeType || "").toLowerCase();
  if (type.includes("intent")) return { step: "intent", text: "正在理解你的问题意图..." };
  if (
    type.includes("rag")
    || type.includes("retrieval")
    || type.includes("query")
    || type.includes("tool")
    || type.includes("standard")
    || type.includes("optimization")
  ) {
    return { step: "retrieval", text: "正在检索证据或调用工具..." };
  }
  if (type.includes("answer") || type.includes("verification")) return { step: "organize", text: "正在整理证据并生成回答..." };
  return { step: "organize", text: "正在推进智能体工作流..." };
}

export default function AgentProgressBar({ progress }: { progress: AgentProgressState }) {
  if (!progress.active) return null;
  const currentIndex = progressToIndex(progress.current);

  return (
    <div className="mx-auto w-full max-w-5xl px-2 pb-5 pt-1">
      <div className="grid grid-cols-5 gap-0">
        {AGENT_PROGRESS_STEPS.map((step, index) => {
          const completed = progress.completed.includes(step.key) || index < currentIndex;
          const active = step.key === progress.current;
          return (
            <div key={step.key} className="flex min-w-0 items-start">
              <div className="flex min-w-0 flex-1 flex-col items-center text-center">
                <div className={`relative flex h-6 w-6 items-center justify-center rounded-full border text-xs font-bold transition-colors ${
                  completed
                    ? "border-indigo-500 bg-white text-indigo-600"
                    : active
                      ? "border-transparent bg-white text-indigo-600"
                      : "border-slate-200 bg-white text-slate-400"
                }`}>
                  {active && (
                    <span className="absolute inset-0 animate-spin rounded-full border border-indigo-500 border-b-transparent border-r-transparent" />
                  )}
                  {completed ? <Check size={14} /> : index + 1}
                </div>
                <div className={`mt-1.5 truncate text-xs font-medium ${
                  completed || active ? "text-slate-700" : "text-slate-400"
                }`}>
                  {step.label}
                </div>
              </div>
              {index < AGENT_PROGRESS_STEPS.length - 1 && (
                <div className={`mt-3 h-px flex-1 ${
                  index < currentIndex ? "bg-indigo-500" : "bg-slate-200"
                }`} />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
