import { useState } from "react";
import { AlertTriangle, Clock, Cpu } from "lucide-react";
import type { WebPendingConfirmation } from "./webTypes";

function readArg(item: WebPendingConfirmation, keys: string[]) {
  for (const key of keys) {
    const value = item.arguments[key];
    if (typeof value === "string" && value.trim()) return value;
    if (typeof value === "number") return String(value);
  }
  return undefined;
}

function ConfirmCard({
  item,
  onConfirm,
}: {
  item: WebPendingConfirmation;
  onConfirm: (item: WebPendingConfirmation, approved: boolean) => void;
}) {
  const danger = item.permission === "danger";
  const [confirmText, setConfirmText] = useState("");
  const unlocked = !danger || confirmText.trim().toLowerCase() === "confirm";
  const estimatedTime = readArg(item, ["estimated_time", "estimated_duration", "duration", "time"]);
  const resource = readArg(item, ["resource_usage", "resource", "cost", "resources"]);
  return (
    <div className={`rounded-lg border bg-white p-3 ${danger ? "border-red-300" : "border-amber-200"}`}>
      <div className="flex items-center gap-2">
        {danger && <AlertTriangle size={16} className="text-red-600" />}
        <div className="font-semibold text-slate-900">{item.title}</div>
        <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${danger ? "bg-red-100 text-red-700" : "bg-amber-100 text-amber-700"}`}>{danger ? "高风险" : "需确认"}</span>
      </div>
      {(item.warning || danger) && <p className={`mt-1 text-sm leading-relaxed ${danger ? "text-red-700" : "text-amber-700"}`}>{item.warning || "此操作无法撤销，请谨慎确认。"}</p>}
      {(estimatedTime || resource) && (
        <div className="mt-2 flex flex-wrap gap-3 text-xs text-slate-500">
          {estimatedTime && <span className="inline-flex items-center gap-1"><Clock size={13} />预估耗时：{estimatedTime}</span>}
          {resource && <span className="inline-flex items-center gap-1"><Cpu size={13} />资源占用：{resource}</span>}
        </div>
      )}
      {danger && (
        <div className="mt-3">
          <label className="block text-xs font-medium text-red-700">请输入 <code className="rounded bg-red-50 px-1 py-0.5 font-mono">confirm</code> 以确认执行</label>
          <input value={confirmText} onChange={(event) => setConfirmText(event.target.value)} placeholder="confirm" className="mt-1 w-full rounded-md border border-red-200 bg-white px-2 py-1.5 text-sm outline-none focus:border-red-400 focus:ring-2 focus:ring-red-100" />
        </div>
      )}
      <div className="mt-3 flex gap-2">
        <button type="button" onClick={() => onConfirm(item, true)} disabled={!unlocked} className={`rounded-md px-3 py-1.5 text-base font-semibold text-white disabled:opacity-40 ${danger ? "bg-red-600 hover:bg-red-700" : "bg-amber-600 hover:bg-amber-700"}`}>确认执行</button>
        <button type="button" onClick={() => onConfirm(item, false)} className="rounded-md border border-slate-200 px-3 py-1.5 text-base text-slate-600 hover:bg-slate-50">取消</button>
      </div>
    </div>
  );
}

export default function WebConfirmDialog({
  confirmations,
  onConfirm,
}: {
  confirmations: WebPendingConfirmation[];
  onConfirm: (item: WebPendingConfirmation, approved: boolean) => void;
}) {
  if (confirmations.length === 0) return null;
  const danger = confirmations.some((item) => item.permission === "danger");
  return (
    <div className={`rounded-xl border p-4 shadow-sm ${danger ? "border-red-200 bg-red-50" : "border-amber-200 bg-amber-50"}`}>
      <div className={`mb-3 flex items-center gap-2 text-base font-semibold ${danger ? "text-red-800" : "text-amber-800"}`}>
        {danger && <AlertTriangle size={16} />}
        需要确认的操作
      </div>
      <div className="space-y-2">
        {confirmations.map((item) => <ConfirmCard key={item.action_id} item={item} onConfirm={onConfirm} />)}
      </div>
    </div>
  );
}
