import { useState } from "react";
import { AlertTriangle, Clock, Cpu } from "lucide-react";
import type { AgentPendingConfirmation } from "./types";

/**
 * P1-3: 高风险操作确认弹窗
 * - danger 级别：红色主题 + "此操作无法撤销" + 需输入 "confirm" 才能执行
 * - confirm 级别：黄色主题 + 简单确认按钮
 * - 显示预估耗时和资源占用（从 arguments 中读取）
 */
interface AgentConfirmDialogProps {
  confirmations: AgentPendingConfirmation[];
  onConfirm: (item: AgentPendingConfirmation, approved: boolean) => void;
}

/** 从 arguments 中安全读取字符串字段 */
function readArg(item: AgentPendingConfirmation, keys: string[]): string | undefined {
  for (const key of keys) {
    const v = item.arguments?.[key];
    if (typeof v === "string" && v.trim()) return v;
    if (typeof v === "number") return String(v);
  }
  return undefined;
}

/** 单条确认卡片 */
function ConfirmCard({
  item,
  onConfirm,
}: {
  item: AgentPendingConfirmation;
  onConfirm: (item: AgentPendingConfirmation, approved: boolean) => void;
}) {
  const isDanger = item.permission === "danger";
  // danger 级别需要用户输入 "confirm" 才能解锁执行按钮
  const [confirmText, setConfirmText] = useState("");
  const dangerUnlocked = !isDanger || confirmText.trim().toLowerCase() === "confirm";

  // 预估耗时 / 资源占用
  const estimatedTime =
    readArg(item, ["estimated_time", "estimated_duration", "duration", "time"]) ?? undefined;
  const resourceUsage =
    readArg(item, ["resource_usage", "resource", "cost", "resources"]) ?? undefined;

  return (
    <div
      className={`rounded-lg border p-3 ${
        isDanger ? "border-red-300 bg-white" : "border-amber-200 bg-white"
      }`}
    >
      <div className="flex items-center gap-2">
        {isDanger && <AlertTriangle size={16} className="text-red-600" />}
        <div className="font-semibold text-slate-900">{item.title}</div>
        <span
          className={`rounded-full px-2 py-0.5 text-xs font-semibold ${
            isDanger ? "bg-red-100 text-red-700" : "bg-amber-100 text-amber-700"
          }`}
        >
          {isDanger ? "高风险" : "需确认"}
        </span>
      </div>

      {(item.warning || isDanger) && (
        <p
          className={`mt-1 text-sm leading-relaxed ${
            isDanger ? "text-red-700" : "text-amber-700"
          }`}
        >
          {item.warning}
          {isDanger && !item.warning ? "此操作无法撤销，请谨慎确认。" : ""}
          {isDanger && item.warning ? " 此操作无法撤销。" : ""}
        </p>
      )}

      {/* 预估耗时与资源占用 */}
      {(estimatedTime || resourceUsage) && (
        <div className="mt-2 flex flex-wrap gap-3 text-xs text-slate-500">
          {estimatedTime && (
            <span className="inline-flex items-center gap-1">
              <Clock size={13} /> 预估耗时：{estimatedTime}
            </span>
          )}
          {resourceUsage && (
            <span className="inline-flex items-center gap-1">
              <Cpu size={13} /> 资源占用：{resourceUsage}
            </span>
          )}
        </div>
      )}

      {/* danger 级别：输入确认词 */}
      {isDanger && (
        <div className="mt-3">
          <label className="block text-xs font-medium text-red-700">
            请输入 <code className="rounded bg-red-50 px-1 py-0.5 font-mono text-red-700">confirm</code> 以确认执行
          </label>
          <input
            value={confirmText}
            onChange={(e) => setConfirmText(e.target.value)}
            autoFocus
            placeholder="confirm"
            className="mt-1 w-full rounded-md border border-red-200 bg-white px-2 py-1.5 text-sm outline-none focus:border-red-400 focus:ring-2 focus:ring-red-100"
          />
        </div>
      )}

      <div className="mt-3 flex gap-2">
        <button
          onClick={() => onConfirm(item, true)}
          disabled={!dangerUnlocked}
          className={`rounded-md px-3 py-1.5 text-base font-semibold text-white transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
            isDanger ? "bg-red-600 hover:bg-red-700" : "bg-amber-600 hover:bg-amber-700"
          }`}
        >
          确认执行
        </button>
        <button
          onClick={() => onConfirm(item, false)}
          className="rounded-md border border-slate-200 px-3 py-1.5 text-base text-slate-600 transition-colors hover:bg-slate-50"
        >
          取消
        </button>
      </div>
    </div>
  );
}

const AgentConfirmDialog = ({ confirmations, onConfirm }: AgentConfirmDialogProps) => {
  if (!confirmations.length) return null;

  // 是否存在 danger 级别，决定整体容器主题
  const hasDanger = confirmations.some((c) => c.permission === "danger");

  return (
    <div
      className={`rounded-xl border p-4 shadow-sm ${
        hasDanger ? "border-red-200 bg-red-50" : "border-amber-200 bg-amber-50"
      }`}
    >
      <div
        className={`mb-3 flex items-center gap-2 text-base font-semibold ${
          hasDanger ? "text-red-800" : "text-amber-800"
        }`}
      >
        {hasDanger && <AlertTriangle size={16} />}
        需要确认的操作
      </div>
      <div className="space-y-2">
        {confirmations.map((item) => (
          <ConfirmCard key={item.action_id} item={item} onConfirm={onConfirm} />
        ))}
      </div>
    </div>
  );
};

export default AgentConfirmDialog;
