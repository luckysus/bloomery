import { useCallback, useEffect, useMemo, useState } from "react";
import type { AgentConversation } from "../../../web-source/src/agent/types";
import AgentHistorySearchDialog from "../../../web-source/src/components/agent/AgentHistorySearchDialog";
import AgentSidebar from "../../../web-source/src/components/agent/AgentSidebar";
import type { HistoryHit } from "../../bridge/desktop";
import type { SectionId } from "../../app/navigation";
import type { ChatControllerProps } from "./chatController";
import WebAgentChatPanel from "./WebAgentChatPanel";

export interface WebChatWorkspaceProps extends ChatControllerProps {
  onOpenSection?: (section: SectionId) => void;
}

function toAgentConversation(
  conversation: ChatControllerProps["conversations"][number],
): AgentConversation {
  return {
    sessionId: conversation.id,
    title: conversation.title,
    updatedAt: conversation.updated_at,
    messages: [],
    response: null,
    pinned: conversation.pinned,
  };
}

function historyHitToConversation(hit: HistoryHit): AgentConversation {
  return {
    sessionId: hit.conversation_id,
    title: hit.conversation_title,
    updatedAt: hit.created_at,
    messages: [{
      role: hit.role === "user" ? "user" : "agent",
      content: hit.snippet || hit.content,
    }],
    response: null,
  };
}

export default function WebChatWorkspace({
  onOpenSection: _onOpenSection,
  ...controller
}: WebChatWorkspaceProps) {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [recentChatsOpen, setRecentChatsOpen] = useState(false);
  const [renamingConversationId, setRenamingConversationId] = useState<string | null>(null);
  const [renamingConversationTitle, setRenamingConversationTitle] = useState("");
  const [conversationMenuId, setConversationMenuId] = useState<string | null>(null);
  const [historySearchOpen, setHistorySearchOpen] = useState(false);
  const [historySearch, setHistorySearch] = useState("");
  const [historySearchResults, setHistorySearchResults] = useState<AgentConversation[]>([]);

  const agentConversations = useMemo(
    () => controller.conversations.map(toAgentConversation),
    [controller.conversations],
  );

  useEffect(() => {
    if (!historySearchOpen) return;
    const query = historySearch.trim();
    if (!query) {
      setHistorySearchResults([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void controller.onSearchHistory(query).then((hits) => {
        if (!cancelled) setHistorySearchResults(hits.map(historyHitToConversation));
      });
    }, 120);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [controller.onSearchHistory, historySearch, historySearchOpen]);

  useEffect(() => {
    if (!historySearchOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setHistorySearchOpen(false);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [historySearchOpen]);

  const openHistorySearch = useCallback(() => {
    setHistorySearch("");
    setHistorySearchResults([]);
    setHistorySearchOpen(true);
  }, []);

  const closeHistorySearch = useCallback(() => {
    setHistorySearchOpen(false);
    setHistorySearch("");
    setHistorySearchResults([]);
  }, []);

  const beginRename = useCallback((conversation: AgentConversation) => {
    setConversationMenuId(null);
    setRenamingConversationId(conversation.sessionId);
    setRenamingConversationTitle(conversation.title);
  }, []);

  const commitRename = useCallback(() => {
    if (!renamingConversationId) return;
    controller.onRenameConversation(renamingConversationId, renamingConversationTitle);
    setRenamingConversationId(null);
  }, [controller.onRenameConversation, renamingConversationId, renamingConversationTitle]);

  return (
    <section
      className={`bloomery-web-chat-embedded bloomery-web-chat-desktop-clean relative z-10 flex h-full min-h-0 w-full overflow-hidden bg-[#fbf7ef] ${
        sidebarCollapsed ? "is-sidebar-collapsed" : ""
      }`}
      aria-label="钢铁智能体"
    >
      <AgentSidebar
        isAgentMode
        collapsed={sidebarCollapsed}
        recentChatsOpen={recentChatsOpen}
        conversationSyncError=""
        conversations={agentConversations}
        filteredConversations={agentConversations}
        activeSessionId={controller.selectedId ?? ""}
        renamingConversationId={renamingConversationId}
        renamingConversationTitle={renamingConversationTitle}
        conversationMenuId={conversationMenuId}
        profileInitial="D"
        profileUsername="DSL"
        showModeSwitcher={false}
        showProfileMenu={false}
        setRenamingConversationTitle={setRenamingConversationTitle}
        onExpand={() => {
          setSidebarCollapsed(false);
          setRecentChatsOpen(false);
        }}
        onCollapse={() => setSidebarCollapsed(true)}
        onStartNew={() => void controller.onNewConversation()}
        onOpenHistorySearch={openHistorySearch}
        onToggleRecentChats={() => setRecentChatsOpen((open) => !open)}
        onSelectConversation={(conversation) => controller.onSelectConversation(conversation.sessionId)}
        onCommitRename={commitRename}
        onCancelRename={() => setRenamingConversationId(null)}
        onTogglePin={(conversation) => {
          const local = controller.conversations.find((item) => item.id === conversation.sessionId);
          if (local) controller.onToggleConversationPinned(local);
        }}
        onToggleConversationMenu={(conversation) => setConversationMenuId((current) => (
          current === conversation.sessionId ? null : conversation.sessionId
        ))}
        onShareConversation={(conversation) => {
          void navigator.clipboard?.writeText(conversation.title);
          setConversationMenuId(null);
        }}
        onBeginRename={beginRename}
        onDeleteConversation={(conversation) => controller.onDeleteConversation(conversation.sessionId)}
      >
        <span />
      </AgentSidebar>

      <WebAgentChatPanel {...controller} />

      <div className="bloomery-web-chat-history-search-shell">
        <AgentHistorySearchDialog
          open={historySearchOpen}
          search={historySearch}
          conversations={historySearch.trim() ? historySearchResults : agentConversations}
          onSearchChange={setHistorySearch}
          onClose={closeHistorySearch}
          onStartNew={() => {
            closeHistorySearch();
            void controller.onNewConversation();
          }}
          onSelectConversation={(conversation) => {
            closeHistorySearch();
            controller.onSelectConversation(conversation.sessionId);
          }}
        />
      </div>
    </section>
  );
}
