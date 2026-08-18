import { Pencil, X } from "lucide-react";
import type { AgentConversation } from "../../agent/types";

interface AgentHistorySearchDialogProps {
  open: boolean;
  search: string;
  conversations: AgentConversation[];
  onSearchChange: (value: string) => void;
  onClose: () => void;
  onStartNew: () => void;
  onSelectConversation: (conversation: AgentConversation) => void;
}

export default function AgentHistorySearchDialog({
  open,
  search,
  conversations,
  onSearchChange,
  onClose,
  onStartNew,
  onSelectConversation,
}: AgentHistorySearchDialogProps) {
  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center bg-slate-950/20 px-4 pt-[16vh] max-md:px-3 max-md:pt-[8vh]"
      onPointerDown={onClose}
    >
      <div
        className="w-full max-w-2xl overflow-hidden rounded-2xl bg-[#181715] text-white shadow-2xl shadow-slate-950/30 ring-1 ring-white/10"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <div className="flex h-16 items-center border-b border-white/10 px-5">
          <input
            autoFocus
            value={search}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder="搜索聊天..."
            className="min-w-0 flex-1 bg-transparent text-base text-white outline-none placeholder:text-white/45"
          />
          <button
            onClick={onClose}
            className="ml-3 flex h-9 w-9 items-center justify-center rounded-full text-white/60 transition-colors hover:bg-white/10 hover:text-white"
            title="关闭"
          >
            <X size={18} />
          </button>
        </div>
        <div className="max-h-[54vh] overflow-y-auto p-2 [scrollbar-gutter:stable]">
          <button
            onClick={onStartNew}
            className="flex h-11 w-full items-center gap-3 rounded-xl px-4 text-left text-base font-semibold text-white transition-colors hover:bg-white/10"
          >
            <Pencil size={18} className="text-white/85" />
            新聊天
          </button>
          <div className="px-4 pb-2 pt-4 text-sm font-medium text-white/45">
            {search.trim() ? "搜索结果" : "最近"}
          </div>
          {conversations.length === 0 ? (
            <div className="px-4 py-4 text-sm text-white/45">没有匹配的聊天</div>
          ) : (
            <div className="space-y-1 pb-2">
              {conversations.map((conversation) => (
                <button
                  key={conversation.sessionId}
                  onClick={() => onSelectConversation(conversation)}
                  className="flex h-11 w-full items-center rounded-xl px-4 text-left text-base font-medium text-white transition-colors hover:bg-white/10"
                >
                  <span className="truncate">{conversation.title}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
