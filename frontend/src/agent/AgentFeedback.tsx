import { useState } from "react";
import { ThumbsUp, ThumbsDown } from "lucide-react";

/**
 * P3-4: 用户反馈机制
 * 每条 agent 回复下方显示 👍/👎 按钮，负反馈时弹出原因选择。
 */

type Rating = "up" | "down";

/** 负反馈原因选项 */
const DOWN_REASONS: { value: string; label: string }[] = [
  { value: "intent_error", label: "意图识别错误" },
  { value: "evidence_irrelevant", label: "证据不相关" },
  { value: "plan_impractical", label: "方案不实用" },
  { value: "other", label: "其他" },
];

interface AgentFeedbackProps {
  /** 消息唯一标识 */
  messageId: string;
  /** 反馈回调，负反馈会携带原因 */
  onFeedback: (rating: Rating, reason?: string) => void;
}

export default function AgentFeedback({ messageId, onFeedback }: AgentFeedbackProps) {
  const [rating, setRating] = useState<Rating | null>(null);
  const [showReason, setShowReason] = useState(false);
  const [reason, setReason] = useState<string>("");

  // 记录正面反馈
  const handleUp = () => {
    if (rating === "up") return;
    setRating("up");
    setShowReason(false);
    onFeedback("up");
  };

  // 负反馈：展开原因选择
  const handleDown = () => {
    if (rating === "down") {
      // 再次点击收起
      setShowReason((v) => !v);
      return;
    }
    setRating("down");
    setShowReason(true);
    onFeedback("down", undefined);
  };

  // 提交负反馈原因
  const submitReason = () => {
    onFeedback("down", reason || "other");
    setShowReason(false);
  };

  return (
    <div className="mt-2 flex items-center gap-1">
      <button
        type="button"
        onClick={handleUp}
        aria-label="有用"
        title="有用"
        className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors ${
          rating === "up"
            ? "bg-emerald-50 text-emerald-600"
            : "text-slate-400 hover:bg-slate-100 hover:text-slate-700"
        }`}
      >
        <ThumbsUp size={15} />
      </button>
      <button
        type="button"
        onClick={handleDown}
        aria-label="无用"
        title="无用"
        className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors ${
          rating === "down"
            ? "bg-red-50 text-red-600"
            : "text-slate-400 hover:bg-slate-100 hover:text-slate-700"
        }`}
      >
        <ThumbsDown size={15} />
      </button>

      {showReason && (
        <div className="ml-1 flex flex-wrap items-center gap-1.5">
          <span className="text-xs text-slate-500">原因：</span>
          {DOWN_REASONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setReason(opt.value)}
              className={`rounded-full border px-2.5 py-0.5 text-xs transition-colors ${
                reason === opt.value
                  ? "border-red-300 bg-red-50 text-red-700"
                  : "border-slate-200 bg-white text-slate-600 hover:bg-slate-50"
              }`}
            >
              {opt.label}
            </button>
          ))}
          <button
            type="button"
            onClick={submitReason}
            disabled={!reason}
            className="rounded-full bg-slate-900 px-2.5 py-0.5 text-xs font-medium text-white transition-colors hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-40"
          >
            提交
          </button>
        </div>
      )}

      {/* 隐藏的 messageId 引用，便于未来扩展（如本地持久化已反馈状态） */}
      <span data-feedback-message-id={messageId} className="hidden" />
    </div>
  );
}
