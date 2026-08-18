import { useState } from "react";
import { ThumbsDown, ThumbsUp } from "lucide-react";

type Rating = "up" | "down";

const REASONS = [
  ["intent_error", "意图识别错误"],
  ["evidence_irrelevant", "证据不相关"],
  ["plan_impractical", "方案不实用"],
  ["other", "其他"],
] as const;

export default function WebFeedback({
  messageId,
  onFeedback,
}: {
  messageId: string;
  onFeedback?: (rating: Rating, reason?: string) => void;
}) {
  const [rating, setRating] = useState<Rating | null>(null);
  const [reason, setReason] = useState("");
  const [showReason, setShowReason] = useState(false);
  const send = (next: Rating) => {
    setRating(next);
    setShowReason(next === "down");
    onFeedback?.(next);
  };
  return (
    <div className="mt-2 flex flex-wrap items-center gap-1">
      <button type="button" onClick={() => send("up")} aria-label="有用" title="有用" className={`flex h-7 w-7 items-center justify-center rounded-md ${rating === "up" ? "bg-emerald-50 text-emerald-600" : "text-slate-400 hover:bg-slate-100 hover:text-slate-700"}`}>
        <ThumbsUp size={15} />
      </button>
      <button type="button" onClick={() => send("down")} aria-label="无用" title="无用" className={`flex h-7 w-7 items-center justify-center rounded-md ${rating === "down" ? "bg-red-50 text-red-600" : "text-slate-400 hover:bg-slate-100 hover:text-slate-700"}`}>
        <ThumbsDown size={15} />
      </button>
      {showReason && (
        <div className="ml-1 flex flex-wrap items-center gap-1.5">
          <span className="text-xs text-slate-500">原因：</span>
          {REASONS.map(([value, label]) => (
            <button key={value} type="button" onClick={() => setReason(value)} className={`rounded-full border px-2.5 py-0.5 text-xs ${reason === value ? "border-red-300 bg-red-50 text-red-700" : "border-slate-200 bg-white text-slate-600 hover:bg-slate-50"}`}>
              {label}
            </button>
          ))}
          <button type="button" onClick={() => { onFeedback?.("down", reason || "other"); setShowReason(false); }} disabled={!reason} className="rounded-full bg-slate-900 px-2.5 py-0.5 text-xs font-medium text-white disabled:opacity-40">
            提交
          </button>
        </div>
      )}
      <span data-feedback-message-id={messageId} className="hidden" />
    </div>
  );
}
