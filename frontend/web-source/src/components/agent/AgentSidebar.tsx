import { useEffect, useRef, useState } from "react";
import type { Dispatch, KeyboardEvent, ReactNode, SetStateAction } from "react";
import { Atom, BotMessageSquare, Check, ChevronDown, Ellipsis, LogOut, MessageCircle, Pin, Search, Settings, Share, Pencil, Trash2 } from "lucide-react";
import type { AgentConversation } from "../../agent/types";

const SidebarPanelIcon = ({ size = 20, className = "" }: { size?: number; className?: string }) => (
  <svg
    aria-hidden="true"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
  >
    <rect x="4.5" y="5.5" width="15" height="13" rx="3" />
    <path d="M10.5 5.5v13" />
  </svg>
);

const ChatGptNewChatIcon = ({ size = 18, className = "" }: { size?: number; className?: string }) => (
  <svg
    aria-hidden="true"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
  >
    <path d="M12 5H7.5A2.5 2.5 0 0 0 5 7.5v9A2.5 2.5 0 0 0 7.5 19h9A2.5 2.5 0 0 0 19 16.5V12" />
    <path d="M14.2 4.8h5v5" />
    <path d="M19.2 4.8 10.5 13.5" />
    <path d="m9.8 14.2-.8 2.8 2.8-.8" />
  </svg>
);

interface AgentSidebarProps {
  isAgentMode: boolean;
  collapsed: boolean;
  recentChatsOpen: boolean;
  conversationSyncError: string;
  conversations: AgentConversation[];
  filteredConversations: AgentConversation[];
  activeSessionId: string;
  renamingConversationId: string | null;
  renamingConversationTitle: string;
  conversationMenuId: string | null;
  profileInitial: string;
  profileUsername: string;
  setRenamingConversationTitle: Dispatch<SetStateAction<string>>;
  onExpand: () => void;
  onCollapse: () => void;
  onStartNew: () => void;
  onOpenHistorySearch: () => void;
  onToggleRecentChats: () => void;
  onSelectConversation: (conversation: AgentConversation) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onTogglePin: (conversation: AgentConversation) => void;
  onToggleConversationMenu: (conversation: AgentConversation) => void;
  onShareConversation: (conversation: AgentConversation) => void;
  onBeginRename: (conversation: AgentConversation) => void;
  onDeleteConversation: (conversation: AgentConversation) => void;
  onOpenProfile?: () => void;
  onLogout?: () => void;
  onSwitchRetrieval?: () => void;
  onSwitchAgent?: () => void;
  showProfileMenu?: boolean;
  showModeSwitcher?: boolean;
  children: ReactNode;
}

export default function AgentSidebar({
  isAgentMode,
  collapsed,
  recentChatsOpen,
  conversationSyncError,
  conversations,
  filteredConversations,
  activeSessionId,
  renamingConversationId,
  renamingConversationTitle,
  conversationMenuId,
  profileInitial,
  profileUsername,
  setRenamingConversationTitle,
  onExpand,
  onCollapse,
  onStartNew,
  onOpenHistorySearch,
  onToggleRecentChats,
  onSelectConversation,
  onCommitRename,
  onCancelRename,
  onTogglePin,
  onToggleConversationMenu,
  onShareConversation,
  onBeginRename,
  onDeleteConversation,
  onOpenProfile,
  onLogout,
  onSwitchRetrieval,
  onSwitchAgent,
  showProfileMenu = true,
  showModeSwitcher = true,
  children,
}: AgentSidebarProps) {
  const isCollapsed = collapsed;

  return (
    <>
      {/* 窄屏抽屉展开时的遮罩；桌面端不渲染视觉效果 */}
      {!isCollapsed && (
        <div
          className="fixed inset-0 z-40 bg-slate-900/40 md:hidden"
          onClick={onCollapse}
          aria-hidden="true"
        />
      )}
      <aside
        className={`relative z-30 h-full shrink-0 overflow-visible transition-[width] duration-300 ease-out ${isCollapsed ? "w-16" : "w-80"} max-md:fixed max-md:inset-y-0 max-md:left-0 max-md:z-50 max-md:w-80 max-md:transition-transform ${isCollapsed ? "max-md:-translate-x-full" : "max-md:translate-x-0"}`}
      >
      <div className={`flex h-full flex-col border-r border-[#eadfd2] bg-[#f7f1e8] shadow-[inset_-1px_0_0_rgba(255,255,255,0.55)] transition-[width] duration-300 ease-out ${isCollapsed ? "w-16" : "w-80"} max-md:w-80`}>
        <div className="px-3 pt-3 pb-1">
          <div className="flex items-center gap-3 mb-2">
            <button
              type="button"
              onClick={() => {
                if (isCollapsed) onExpand();
              }}
              className={`group relative flex h-10 w-10 shrink-0 items-center justify-center rounded-xl transition-colors ${
                isCollapsed
                  ? "bg-gradient-to-br from-[#8b6f5a] to-[#3f8b75] text-white shadow-lg shadow-[#d9c8b8] hover:from-[#7c5f49] hover:to-[#347762]"
                  : "bg-gradient-to-br from-[#8b6f5a] to-[#3f8b75] shadow-lg shadow-[#d9c8b8]"
              }`}
              title={isCollapsed ? "打开侧栏" : isAgentMode ? "钢铁智能体" : "多模态智能检索"}
            >
              {isCollapsed ? (
                <>
                  <Atom size={20} className="text-white transition-opacity duration-100 group-hover:opacity-0" />
                  <SidebarPanelIcon size={20} className="absolute text-white opacity-0 transition-opacity duration-100 group-hover:opacity-100" />
                  <span className="pointer-events-none absolute left-12 top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg transition-opacity duration-100 group-hover:opacity-100">
                    打开边栏
                  </span>
                </>
              ) : (
                <Atom size={20} className="text-white" />
              )}
            </button>
            {isCollapsed || !showModeSwitcher || !onSwitchRetrieval || !onSwitchAgent ? (
              <div className="min-w-0 flex-1" />
            ) : (
              <ModeSwitcher
                isAgentMode={isAgentMode}
                onSwitchRetrieval={onSwitchRetrieval}
                onSwitchAgent={onSwitchAgent}
              />
            )}
            <button
              onClick={onCollapse}
              className={`ml-auto flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-[#6f6258] transition-all duration-150 hover:bg-[#fffaf3] hover:text-[#2b2118] ${collapsed ? "pointer-events-none opacity-0" : "opacity-100"}`}
              title="关闭侧栏"
            >
              <SidebarPanelIcon size={20} />
            </button>
          </div>
        </div>

        <div className={`flex-1 space-y-3 px-3 ${isCollapsed ? "overflow-visible" : "overflow-hidden"}`}>
          {isAgentMode ? (
            <div className="flex h-full min-h-0 flex-col pb-4">
              <div className={`space-y-1.5 ${collapsed ? "flex flex-col items-center" : ""}`}>
                <SidebarActionButton collapsed={collapsed} title="新聊天" icon={<ChatGptNewChatIcon size={18} className="shrink-0 text-slate-600" />} onClick={onStartNew} />
                <SidebarActionButton collapsed={collapsed} title="搜索聊天" icon={<Search size={18} className="shrink-0 text-slate-600" />} onClick={onOpenHistorySearch} />
                {collapsed && (
                  <button
                    onClick={onToggleRecentChats}
                    className={`group relative flex h-10 w-10 items-center justify-center rounded-lg text-[#6f6258] transition-colors hover:bg-[#fffaf3] ${
                      recentChatsOpen ? "bg-[#fffaf3]" : ""
                    }`}
                    title="最近聊天"
                  >
                    <MessageCircle size={18} className="shrink-0 text-slate-600" />
                    <span className="pointer-events-none absolute left-12 top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg transition-opacity duration-100 group-hover:opacity-100">
                      最近聊天
                    </span>
                  </button>
                )}
              </div>

              <div className={`mt-5 min-h-0 flex-1 overflow-y-auto pr-1 transition-opacity duration-150 [scrollbar-gutter:stable] ${collapsed ? "pointer-events-none opacity-0" : "opacity-100"}`}>
                <div className="mb-2 px-3 text-sm font-bold text-[#7d7065]">最近</div>
                {conversationSyncError && (
                  <div className="mb-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs leading-relaxed text-amber-700">
                    {conversationSyncError}
                  </div>
                )}
                {conversations.length === 0 ? (
                  <div className="px-3 py-2 text-sm leading-relaxed text-[#a39384]">
                    暂无聊天记录
                  </div>
                ) : (
                  <div className="space-y-0.5">
                    {filteredConversations.map((conversation) => {
                      const active = conversation.sessionId === activeSessionId;
                      return (
                        <div
                          key={conversation.sessionId}
                          onClick={() => {
                            if (renamingConversationId !== conversation.sessionId) {
                              onSelectConversation(conversation);
                            }
                          }}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(event: KeyboardEvent<HTMLDivElement>) => {
                            if (renamingConversationId === conversation.sessionId) return;
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              onSelectConversation(conversation);
                            }
                          }}
                          className={`group relative flex h-9 w-full cursor-pointer items-center rounded-lg px-3 text-left text-sm font-medium transition-colors ${
                            active
                              ? "bg-[#fffaf3] text-[#2b2118] shadow-sm"
                              : "text-[#5b5048] hover:bg-[#fffaf3] hover:text-[#2b2118]"
                          }`}
                        >
                          {renamingConversationId === conversation.sessionId ? (
                            <input
                              autoFocus
                              value={renamingConversationTitle}
                              onChange={(event) => setRenamingConversationTitle(event.target.value)}
                              onBlur={onCommitRename}
                              onKeyDown={(event) => {
                                if (event.key === "Enter") onCommitRename();
                                if (event.key === "Escape") onCancelRename();
                              }}
                              className="min-w-0 flex-1 rounded-md border border-indigo-200 bg-white px-2 py-1 text-sm outline-none ring-2 ring-indigo-50"
                            />
                          ) : (
                            <div className="min-w-0 flex-1 truncate text-left">
                              {conversation.title}
                            </div>
                          )}
                          <div className={`ml-2 flex shrink-0 items-center gap-0.5 transition-opacity ${
                            active || conversationMenuId === conversation.sessionId || conversation.pinned
                              ? "opacity-100"
                              : "opacity-0 group-hover:opacity-100"
                          }`}>
                            <button
                              onClick={(event) => {
                                event.stopPropagation();
                                onTogglePin(conversation);
                              }}
                              className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors hover:bg-[#efe5da] ${
                                conversation.pinned ? "text-[#cc785c]" : "text-[#7d7065]"
                              }`}
                              title={conversation.pinned ? "取消置顶" : "置顶聊天"}
                            >
                              <Pin size={15} className={conversation.pinned ? "" : "rotate-45"} />
                            </button>
                            <button
                              onClick={(event) => {
                                event.stopPropagation();
                                onToggleConversationMenu(conversation);
                              }}
                              className="flex h-7 w-7 items-center justify-center rounded-md text-[#7d7065] transition-colors hover:bg-[#efe5da]"
                              title="更多操作"
                            >
                              <Ellipsis size={17} />
                            </button>
                          </div>
                          {conversationMenuId === conversation.sessionId && (
                            <ConversationMenu
                              conversation={conversation}
                              onShareConversation={onShareConversation}
                              onBeginRename={onBeginRename}
                              onTogglePin={onTogglePin}
                              onDeleteConversation={onDeleteConversation}
                            />
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {showProfileMenu && onOpenProfile && onLogout ? (
                <ProfileMenuButton
                  collapsed={collapsed}
                  profileInitial={profileInitial}
                  profileUsername={profileUsername}
                  onOpenProfile={onOpenProfile}
                  onLogout={onLogout}
                />
              ) : null}
            </div>
          ) : (
            <div className={`h-full space-y-3 overflow-y-auto overscroll-contain pb-4 transition-opacity duration-200 ease-out ${collapsed ? "pointer-events-none w-0 opacity-0" : "w-[296px] opacity-100 max-md:w-full"}`}>
              {!collapsed && children}
            </div>
          )}
        </div>
      </div>
      </aside>
    </>
  );
}

function ProfileMenuButton({
  collapsed,
  profileInitial,
  profileUsername,
  onOpenProfile,
  onLogout,
}: {
  collapsed: boolean;
  profileInitial: string;
  profileUsername: string;
  onOpenProfile: () => void;
  onLogout: () => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div ref={ref} className="relative mt-3">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        className={`group relative flex items-center rounded-xl text-left transition-colors ${
          collapsed
            ? "h-11 w-10 justify-center px-0 py-0 hover:bg-[#fffaf3]"
            : "w-full gap-3 px-2 py-3 hover:bg-[#fffaf3]"
        } ${open ? "bg-[#fffaf3]" : ""}`}
        title="账户"
      >
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-[#e7ddd2] text-sm font-bold text-[#2b2118] transition-colors hover:bg-[#ded0c2]">
          {profileInitial}
        </span>
        <span className={`min-w-0 flex-1 transition-opacity duration-100 ${collapsed ? "hidden" : "opacity-100"}`}>
          <span className="block truncate text-sm font-semibold text-[#2b2118]">{profileUsername}</span>
          <span className="block truncate text-xs text-[#7d7065]">账户与设置</span>
        </span>
        {!collapsed && <Ellipsis size={18} className="shrink-0 text-[#7d7065]" />}
        {collapsed && (
          <span className="pointer-events-none absolute left-12 top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg transition-opacity duration-100 group-hover:opacity-100">
            账户
          </span>
        )}
      </button>
      {open && (
        <div className="absolute bottom-full left-0 z-50 mb-2 w-56 rounded-xl border border-slate-200 bg-white p-1.5 shadow-xl shadow-slate-900/10">
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              onOpenProfile();
            }}
            className="flex h-10 w-full items-center gap-3 rounded-lg px-3 text-sm font-medium text-slate-700 transition-colors hover:bg-slate-100"
          >
            <Settings size={18} className="shrink-0 text-slate-500" />
            系统设置
          </button>
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              onLogout();
            }}
            className="flex h-10 w-full items-center gap-3 rounded-lg px-3 text-sm font-medium text-red-600 transition-colors hover:bg-red-50"
          >
            <LogOut size={18} className="shrink-0" />
            退出登录
          </button>
        </div>
      )}
    </div>
  );
}

function ModeSwitcher({
  isAgentMode,
  onSwitchRetrieval,
  onSwitchAgent,
}: {
  isAgentMode: boolean;
  onSwitchRetrieval: () => void;
  onSwitchAgent: () => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const options = [
    {
      key: "agent",
      label: "钢铁智能体",
      description: "自动规划、调用工具、汇总证据",
      icon: <BotMessageSquare size={18} className="shrink-0" />,
      active: isAgentMode,
      onSelect: onSwitchAgent,
    },
    {
      key: "retrieval",
      label: "多模态智能检索",
      description: "多模态检索、成分建议、工艺优化",
      icon: <Search size={18} className="shrink-0" />,
      active: !isAgentMode,
      onSelect: onSwitchRetrieval,
    },
  ];

  return (
    <div ref={ref} className="relative min-w-0 flex-1">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        className="flex max-w-full items-center gap-1.5 rounded-lg px-2 py-1.5 text-left text-xl font-bold text-[#2b2118] transition-colors hover:bg-[#fffaf3]"
      >
        <span className="truncate">{isAgentMode ? "钢铁智能体" : "多模态智能检索"}</span>
        <ChevronDown size={16} className={`shrink-0 text-[#7d7065] transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute left-0 top-full z-50 mt-1 w-64 rounded-2xl border border-[#eadfd2] bg-white p-1.5 shadow-xl shadow-[#d9c8b8]/40">
          {options.map((option) => (
            <button
              key={option.key}
              type="button"
              onClick={() => {
                if (!option.active) option.onSelect();
                setOpen(false);
              }}
              className={`flex w-full items-start gap-3 rounded-xl px-3 py-2.5 text-left transition-colors ${
                option.active ? "bg-[#f7f1e8]" : "hover:bg-[#f7f1e8]"
              }`}
            >
              <span className="mt-0.5 text-[#6f6258]">{option.icon}</span>
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-semibold text-[#2b2118]">{option.label}</span>
                <span className="block truncate text-xs text-[#7d7065]">{option.description}</span>
              </span>
              {option.active && <Check size={16} className="mt-1 shrink-0 text-[#3f8b75]" />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function SidebarActionButton({
  collapsed,
  title,
  icon,
  onClick,
}: {
  collapsed: boolean;
  title: string;
  icon: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`group relative flex h-10 items-center rounded-lg text-left text-base font-semibold text-[#5b5048] transition-colors ${
        collapsed
          ? "w-10 justify-center px-0 hover:bg-[#fffaf3]"
          : "w-full gap-3 px-2 hover:bg-[#fffaf3]"
      }`}
      title={title}
    >
      {icon}
      <span className={`whitespace-nowrap transition-opacity duration-100 ${collapsed ? "hidden" : "opacity-100"}`}>{title}</span>
      {collapsed && (
        <span className="pointer-events-none absolute left-12 top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg transition-opacity duration-100 group-hover:opacity-100">
          {title}
        </span>
      )}
    </button>
  );
}

function ConversationMenu({
  conversation,
  onShareConversation,
  onBeginRename,
  onTogglePin,
  onDeleteConversation,
}: {
  conversation: AgentConversation;
  onShareConversation: (conversation: AgentConversation) => void;
  onBeginRename: (conversation: AgentConversation) => void;
  onTogglePin: (conversation: AgentConversation) => void;
  onDeleteConversation: (conversation: AgentConversation) => void;
}) {
  return (
    <div className="bloomery-web-chat-session-menu absolute right-2 top-8 z-30 w-44 rounded-xl border border-slate-200 bg-white p-1.5 shadow-xl shadow-slate-900/10">
      <MenuButton icon={<Share size={16} />} onClick={(event) => {
        event.stopPropagation();
        onShareConversation(conversation);
      }}>
        分享
      </MenuButton>
      <MenuButton icon={<Pencil size={16} />} onClick={(event) => {
        event.stopPropagation();
        onBeginRename(conversation);
      }}>
        重命名
      </MenuButton>
      <MenuButton icon={<Pin size={16} className={conversation.pinned ? "" : "rotate-45"} />} onClick={(event) => {
        event.stopPropagation();
        onTogglePin(conversation);
      }}>
        {conversation.pinned ? "取消置顶" : "置顶聊天"}
      </MenuButton>
      <div className="bloomery-web-chat-session-menu-divider my-1 h-px bg-slate-100" />
      <button
        onClick={(event) => {
          event.stopPropagation();
          onDeleteConversation(conversation);
        }}
        className="bloomery-web-chat-session-menu-danger flex h-9 w-full items-center gap-3 rounded-lg px-3 text-sm font-medium text-red-600 transition-colors hover:bg-red-50"
      >
        <Trash2 size={16} />
        删除
      </button>
    </div>
  );
}

function MenuButton({
  icon,
  onClick,
  children,
}: {
  icon: ReactNode;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="bloomery-web-chat-session-menu-item flex h-9 w-full items-center gap-3 rounded-lg px-3 text-sm font-medium text-slate-700 transition-colors hover:bg-slate-100"
    >
      {icon}
      {children}
    </button>
  );
}
