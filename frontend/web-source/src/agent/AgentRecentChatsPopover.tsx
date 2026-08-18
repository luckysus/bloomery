import { Pin } from "lucide-react";
import type { AgentConversation } from "./types";

interface AgentRecentChatsPopoverProps {
  conversations: AgentConversation[];
  activeSessionId: string;
  onClose: () => void;
  onSelectConversation: (conversation: AgentConversation) => void;
}

export default function AgentRecentChatsPopover({
  conversations,
  activeSessionId,
  onClose,
  onSelectConversation,
}: AgentRecentChatsPopoverProps) {
  return (
    <div
      className="fixed inset-0 z-[60] bg-transparent max-md:bg-slate-900/30"
      onPointerDown={onClose}
    >
      <div
        className="absolute left-[68px] top-[122px] w-72 overflow-hidden rounded-2xl bg-[#181715] p-2 text-white shadow-2xl shadow-slate-950/25 ring-1 ring-white/10 max-md:left-3 max-md:right-3 max-md:top-[76px] max-md:w-auto"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <div className="px-3 pb-2 pt-2 text-sm font-semibold text-white/65">最近聊天</div>
        <div className="max-h-[52vh] overflow-y-auto [scrollbar-gutter:stable]">
          {conversations.length === 0 ? (
            <div className="px-3 py-4 text-sm text-white/45">暂无聊天记录</div>
          ) : (
            <div className="space-y-1 pb-1">
              {conversations.map((conversation) => (
                <button
                  key={conversation.sessionId}
                  onClick={() => onSelectConversation(conversation)}
                  className={`flex h-11 w-full items-center rounded-xl px-3 text-left text-sm font-medium transition-colors ${
                    conversation.sessionId === activeSessionId
                      ? "bg-white/12 text-white"
                      : "text-white/90 hover:bg-white/10"
                  }`}
                >
                  <span className="min-w-0 flex-1 truncate">{conversation.title}</span>
                  {conversation.pinned && <Pin size={14} className="ml-2 shrink-0 text-white/55" />}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
