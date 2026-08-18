import { Sparkles } from "lucide-react";
import type { AgentRecommendation } from "./types";

function formatAgentValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "number") {
    return Number.isInteger(value) ? String(value) : value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
  }
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

function AgentKeyValueGrid({ data }: { data?: Record<string, unknown> }) {
  const entries = Object.entries(data ?? {}).filter(([, value]) => value !== null && value !== undefined && value !== "");
  if (!entries.length) return null;
  return (
    <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
      {entries.slice(0, 8).map(([key, value]) => (
        <div key={key} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
          <div className="text-xs font-semibold text-slate-400">{key}</div>
          <div className="mt-0.5 break-words text-sm font-semibold text-slate-700">{formatAgentValue(value)}</div>
        </div>
      ))}
    </div>
  );
}

export default function AgentRecommendationCard({ item }: { item: AgentRecommendation }) {
  const details = item.details ?? {};
  const process = details.process as Record<string, unknown> | undefined;
  const predicted = details.predicted_performance as Record<string, unknown> | undefined;
  const factors = Array.isArray(details.factors) ? details.factors.slice(0, 3) as Record<string, unknown>[] : [];
  const nextChecks = Array.isArray(details.next_checks) ? details.next_checks.slice(0, 3) as string[] : [];

  return (
    <div className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h4 className="text-base font-bold text-slate-900">{item.title}</h4>
          {item.category && <div className="mt-0.5 text-xs font-semibold uppercase tracking-wide text-slate-400">{item.category}</div>}
        </div>
        <Sparkles size={16} className="mt-1 shrink-0 text-indigo-500" />
      </div>
      {item.summary && <p className="mt-2 text-base leading-relaxed text-slate-700">{item.summary}</p>}
      {predicted && <AgentKeyValueGrid data={predicted} />}
      {process && <AgentKeyValueGrid data={process} />}
      {factors.length > 0 && (
        <div className="mt-3 space-y-2">
          {factors.map((factor, idx) => (
            <div key={idx} className="rounded-lg border border-amber-100 bg-amber-50 px-3 py-2 text-sm leading-relaxed text-amber-800">
              <span className="font-semibold">{formatAgentValue(factor.field)}</span>
              <span>：低性能组 {formatAgentValue(factor.low_group_avg)}，参考组 {formatAgentValue(factor.reference_group_avg)}，差值 {formatAgentValue(factor.delta)}</span>
            </div>
          ))}
        </div>
      )}
      {nextChecks.length > 0 && (
        <div className="mt-3 space-y-1.5">
          {nextChecks.map((check, idx) => (
            <div key={idx} className="text-sm leading-relaxed text-slate-600">{idx + 1}. {check}</div>
          ))}
        </div>
      )}
      {(item.risks ?? []).length > 0 && (
        <div className="mt-3 rounded-lg border border-red-100 bg-red-50 px-3 py-2 text-sm leading-relaxed text-red-700">
          {item.risks!.slice(0, 2).join("；")}
        </div>
      )}
    </div>
  );
}
