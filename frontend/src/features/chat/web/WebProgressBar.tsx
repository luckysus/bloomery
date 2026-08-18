import { Check } from "lucide-react";

export type WebProgressStepKey = "analysis" | "intent" | "retrieval" | "organize" | "answer";

export interface WebProgressState {
  active: boolean;
  current: WebProgressStepKey;
  completed: WebProgressStepKey[];
  statusText: string;
}

const STEPS: Array<{ key: WebProgressStepKey; label: string }> = [
  { key: "analysis", label: "问题分析" },
  { key: "intent", label: "意图理解" },
  { key: "retrieval", label: "检索/工具" },
  { key: "organize", label: "内容整理" },
  { key: "answer", label: "完成回答" },
];

export function progressFromRunState(state: string | null): WebProgressState {
  if (!state || ["created", "completed", "cancelled", "failed", "interrupted"].includes(state)) {
    return { active: false, current: "analysis", completed: [], statusText: "" };
  }
  if (state === "preparing") return { active: true, current: "analysis", completed: [], statusText: "正在分析问题..." };
  if (state === "awaiting_permission") return { active: true, current: "retrieval", completed: ["analysis", "intent"], statusText: "等待确认工具操作..." };
  if (state === "executing_tools") return { active: true, current: "retrieval", completed: ["analysis", "intent"], statusText: "正在检索证据或调用工具..." };
  if (state === "verifying" || state === "completing") return { active: true, current: "organize", completed: ["analysis", "intent", "retrieval"], statusText: "正在整理证据并生成回答..." };
  return { active: true, current: "answer", completed: ["analysis", "intent", "retrieval", "organize"], statusText: "正在生成回答..." };
}

export default function WebProgressBar({ progress }: { progress: WebProgressState }) {
  if (!progress.active) return null;
  const currentIndex = STEPS.findIndex((step) => step.key === progress.current);
  return (
    <div className="mx-auto w-full max-w-5xl px-2 pb-5 pt-1" aria-label="智能体工作流进度">
      <div className="grid grid-cols-5 gap-0">
        {STEPS.map((step, index) => {
          const completed = progress.completed.includes(step.key) || index < currentIndex;
          const active = step.key === progress.current;
          return (
            <div key={step.key} className="flex min-w-0 items-start">
              <div className="flex min-w-0 flex-1 flex-col items-center text-center">
                <div className={`relative flex h-6 w-6 items-center justify-center rounded-full border text-xs font-bold ${
                  completed ? "border-indigo-500 bg-white text-indigo-600" : active ? "border-transparent bg-white text-indigo-600" : "border-slate-200 bg-white text-slate-400"
                }`}>
                  {active && <span className="absolute inset-0 animate-spin rounded-full border border-indigo-500 border-b-transparent border-r-transparent" />}
                  {completed ? <Check size={14} /> : index + 1}
                </div>
                <div className={`mt-1.5 truncate text-xs font-medium ${completed || active ? "text-slate-700" : "text-slate-400"}`}>
                  {step.label}
                </div>
              </div>
              {index < STEPS.length - 1 && <div className={`mt-3 h-px flex-1 ${index < currentIndex ? "bg-indigo-500" : "bg-slate-200"}`} />}
            </div>
          );
        })}
      </div>
      <div className="mt-2 text-center text-xs text-slate-500">{progress.statusText}</div>
    </div>
  );
}
