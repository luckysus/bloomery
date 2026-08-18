import { Sparkles } from "lucide-react";
import type { WebRecommendation } from "./webTypes";

function valueText(value: unknown) {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "number") return Number.isInteger(value) ? String(value) : value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
  return typeof value === "string" ? value : JSON.stringify(value);
}

export default function WebRecommendationCard({ item }: { item: WebRecommendation }) {
  const entries = Object.entries(item.details ?? {}).filter(([, value]) => value !== null && value !== undefined && value !== "").slice(0, 8);
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
      {entries.length > 0 && (
        <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
          {entries.map(([key, value]) => (
            <div key={key} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
              <div className="text-xs font-semibold text-slate-400">{key}</div>
              <div className="mt-0.5 break-words text-sm font-semibold text-slate-700">{valueText(value)}</div>
            </div>
          ))}
        </div>
      )}
      {(item.risks ?? []).length > 0 && <div className="mt-3 rounded-lg border border-red-100 bg-red-50 px-3 py-2 text-sm leading-relaxed text-red-700">{item.risks?.slice(0, 2).join("；")}</div>}
    </div>
  );
}
