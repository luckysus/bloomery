import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { AgentConversation, AgentMessage, AgentResponse } from "../agent/types";
import { initialAgentProgress, type AgentProgressState } from "../agent/AgentProgressBar";
import {
  archiveConversation as archiveDesktopConversation,
  listConversations as listDesktopConversations,
  listMessages as listDesktopMessages,
  saveConversationSnapshot as saveDesktopConversationSnapshot,
  searchHistory as searchDesktopHistory,
  updateConversationPinned as updateDesktopConversationPinned,
  updateConversationTitle as updateDesktopConversationTitle,
  type DesktopConversation,
  type DesktopHistoryHit,
  type DesktopMessage,
} from "../desktop/services/conversations";
import { isTauriRuntime } from "../desktop/services/tauri";
import { API_BASE, authHeaders } from "../services/api";
import {
  buildAgentConversationTitle,
  normalizeAgentConversation,
  saveAgentConversationRemote,
} from "../services/agentConversations";

function parseDesktopResponse(raw?: string | null): AgentResponse | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw) as AgentResponse;
  } catch {
    return null;
  }
}

function normalizeDesktopMessage(message: DesktopMessage): AgentMessage | null {
  if (message.role === "system") return null;
  const response = parseDesktopResponse(message.response_json);
  return {
    role: message.role === "user" ? "user" : "agent",
    content: message.content,
    ...(response ? { response } : {}),
  };
}

function snapshotAgentMessages(messages: AgentMessage[]) {
  return messages
    .filter((message) => message.content.trim())
    .map((message) => ({
      role: message.role,
      content: message.content,
      response_json: message.response ? JSON.stringify(message.response) : null,
    }));
}

async function loadDesktopAgentConversation(row: DesktopConversation): Promise<AgentConversation> {
  const messages = (await listDesktopMessages(row.id))
    .map(normalizeDesktopMessage)
    .filter((message): message is AgentMessage => Boolean(message));
  const response = [...messages].reverse().find(message => message.response)?.response ?? null;
  return {
    sessionId: row.id,
    title: row.title,
    updatedAt: row.updated_at,
    messages,
    response,
    pinned: row.pinned,
  };
}

async function loadDesktopAgentConversationFromHit(hit: DesktopHistoryHit): Promise<AgentConversation> {
  const messages = (await listDesktopMessages(hit.conversation_id))
    .map(normalizeDesktopMessage)
    .filter((message): message is AgentMessage => Boolean(message));
  const response = [...messages].reverse().find(message => message.response)?.response ?? null;
  const updatedAt = messages.length
    ? hit.created_at
    : new Date().toISOString();
  return {
    sessionId: hit.conversation_id,
    title: hit.conversation_title,
    updatedAt,
    messages,
    response,
  };
}

type UseAgentConversationsArgs = {
  authChecked: boolean;
  isAuthenticated: boolean;
  authUserKey: string;
  agentLoading: boolean;
  setAgentProgress: Dispatch<SetStateAction<AgentProgressState>>;
  setAgentError: Dispatch<SetStateAction<string>>;
  setQuery: Dispatch<SetStateAction<string>>;
  setAgentScrollToBottomKey: Dispatch<SetStateAction<number>>;
};

export function useAgentConversations({
  authChecked,
  isAuthenticated,
  authUserKey,
  agentLoading,
  setAgentProgress,
  setAgentError,
  setQuery,
  setAgentScrollToBottomKey,
}: UseAgentConversationsArgs) {
  const [agentSessionId, setAgentSessionId] = useState("");
  const [agentMessages, setAgentMessages] = useState<AgentMessage[]>([]);
  const [agentResponse, setAgentResponse] = useState<AgentResponse | null>(null);
  const [agentConversations, setAgentConversations] = useState<AgentConversation[]>([]);
  const [agentHistorySearchOpen, setAgentHistorySearchOpen] = useState(false);
  const [agentHistorySearch, setAgentHistorySearch] = useState("");
  const [agentRecentChatsOpen, setAgentRecentChatsOpen] = useState(false);
  const [agentModelMenuOpen, setAgentModelMenuOpen] = useState(false);
  const [agentConversationSyncError, setAgentConversationSyncError] = useState("");
  const [agentConversationMenuId, setAgentConversationMenuId] = useState<string | null>(null);
  const [renamingConversationId, setRenamingConversationId] = useState<string | null>(null);
  const [renamingConversationTitle, setRenamingConversationTitle] = useState("");
  const [desktopSearchConversations, setDesktopSearchConversations] = useState<AgentConversation[]>([]);
  const [copiedAgentMessageIndex, setCopiedAgentMessageIndex] = useState<number | null>(null);
  const [editingAgentMessageIndex, setEditingAgentMessageIndex] = useState<number | null>(null);
  const [editingAgentMessageText, setEditingAgentMessageText] = useState("");
  // 窄屏（<md）首屏默认收起侧栏，避免抽屉盖住内容；桌面端保持展开。
  const [agentSidebarCollapsed, setAgentSidebarCollapsed] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(max-width: 767px)").matches,
  );
  const conversationOwnerKey = isAuthenticated ? authUserKey.trim() : "";
  const conversationOwnerKeyRef = useRef<string | null>(null);

  useEffect(() => {
    if (!authChecked) return;
    if (conversationOwnerKeyRef.current === conversationOwnerKey) return;
    conversationOwnerKeyRef.current = conversationOwnerKey;
    setAgentSessionId("");
    setAgentMessages([]);
    setAgentResponse(null);
    setAgentConversations([]);
    setAgentConversationSyncError("");
    setAgentConversationMenuId(null);
    setRenamingConversationId(null);
    setRenamingConversationTitle("");
    setAgentHistorySearch("");
    setAgentRecentChatsOpen(false);
    setAgentHistorySearchOpen(false);
    setCopiedAgentMessageIndex(null);
    setEditingAgentMessageIndex(null);
    setEditingAgentMessageText("");
    setAgentProgress(initialAgentProgress);
    setAgentError("");
    setQuery("");
  }, [authChecked, conversationOwnerKey, setAgentError, setAgentProgress, setQuery]);

  useEffect(() => {
    if (!authChecked || !isAuthenticated || !conversationOwnerKey) return;
    const ownerKeyAtLoad = conversationOwnerKey;
    let cancelled = false;
    async function loadRemoteAgentConversations() {
      try {
        if (isTauriRuntime()) {
          const rows = await listDesktopConversations();
          const local = await Promise.all(rows.slice(0, 80).map(loadDesktopAgentConversation));
          if (!cancelled && conversationOwnerKeyRef.current === ownerKeyAtLoad) {
            setAgentConversations(local);
            setAgentConversationSyncError("");
          }
          return;
        }
        const resp = await fetch(`${API_BASE}/api/agent/conversations?limit=80`, {
          credentials: "include",
          headers: authHeaders(),
        });
        if (!resp.ok) throw new Error(await resp.text());
        const rows = await resp.json();
        if (!Array.isArray(rows) || cancelled) return;
        const remote = rows
          .map(normalizeAgentConversation)
          .filter((item): item is AgentConversation => Boolean(item));
        if (conversationOwnerKeyRef.current !== ownerKeyAtLoad) return;
        setAgentConversations(
          remote
            .sort((a, b) => Number(Boolean(b.pinned)) - Number(Boolean(a.pinned)) || new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
            .slice(0, 80)
        );
        setAgentConversationSyncError("");
      } catch (err: any) {
        if (!cancelled && conversationOwnerKeyRef.current === ownerKeyAtLoad) {
          setAgentConversationSyncError("会话历史暂时只保存在当前浏览器，后端同步失败。");
          console.warn("加载后端智能体会话失败", err);
        }
      }
    }
    loadRemoteAgentConversations();
    return () => {
      cancelled = true;
    };
  }, [authChecked, isAuthenticated, conversationOwnerKey]);

  const persistAgentConversation = useCallback((
    sessionId: string,
    messages: AgentMessage[],
    response: AgentResponse | null,
  ) => {
    const ownerKeyAtSave = conversationOwnerKey;
    if (!ownerKeyAtSave || conversationOwnerKeyRef.current !== ownerKeyAtSave) return;
    if (!sessionId || messages.length === 0) return;
    let savedConversation: AgentConversation | null = null;
    setAgentConversations(prev => {
      if (conversationOwnerKeyRef.current !== ownerKeyAtSave) return prev;
      const existing = prev.find(item => item.sessionId === sessionId);
      const nextItem: AgentConversation = {
        sessionId,
        title: buildAgentConversationTitle(messages),
        updatedAt: new Date().toISOString(),
        messages,
        response,
        pinned: existing?.pinned,
      };
      savedConversation = nextItem;
      return [nextItem, ...prev.filter(item => item.sessionId !== sessionId)].slice(0, 30);
    });
    window.setTimeout(() => {
      if (!savedConversation) return;
      if (conversationOwnerKeyRef.current !== ownerKeyAtSave) return;
      if (isTauriRuntime()) {
        saveDesktopConversationSnapshot(
          savedConversation.sessionId,
          savedConversation.title,
          snapshotAgentMessages(savedConversation.messages),
        )
          .then(() => setAgentConversationSyncError(""))
          .catch((err) => {
            setAgentConversationSyncError("保存本地桌面会话失败。");
            console.warn("保存本地桌面会话失败", err);
          });
        return;
      }
      setAgentConversationSyncError("");
      saveAgentConversationRemote(savedConversation)
        .then(() => setAgentConversationSyncError(""))
        .catch((err) => {
          setAgentConversationSyncError("会话已临时保存在浏览器，但同步到服务器失败。");
          console.warn("保存智能体会话到后端失败", err);
        });
    }, 0);
  }, [conversationOwnerKey]);

  const startNewAgentConversation = useCallback(() => {
    setAgentSessionId("");
    setAgentMessages([]);
    setAgentResponse(null);
    setAgentProgress(initialAgentProgress);
    setAgentError("");
    setQuery("");
    setCopiedAgentMessageIndex(null);
    setEditingAgentMessageIndex(null);
    setEditingAgentMessageText("");
    setAgentRecentChatsOpen(false);
    setAgentHistorySearchOpen(false);
  }, [setAgentError, setAgentProgress, setQuery]);

  const selectAgentConversation = useCallback((conversation: AgentConversation) => {
    setAgentSessionId(conversation.sessionId);
    setAgentMessages(conversation.messages);
    setAgentResponse(conversation.response);
    setAgentProgress(initialAgentProgress);
    setAgentError("");
    setAgentConversationMenuId(null);
    setCopiedAgentMessageIndex(null);
    setEditingAgentMessageIndex(null);
    setEditingAgentMessageText("");
    setAgentRecentChatsOpen(false);
    setAgentHistorySearchOpen(false);
    setAgentScrollToBottomKey(value => value + 1);
  }, [setAgentError, setAgentProgress, setAgentScrollToBottomKey]);

  const updateAgentConversations = useCallback((updater: (items: AgentConversation[]) => AgentConversation[]) => {
    const ownerKeyAtUpdate = conversationOwnerKey;
    if (!ownerKeyAtUpdate || conversationOwnerKeyRef.current !== ownerKeyAtUpdate) return;
    setAgentConversations(prev => {
      if (conversationOwnerKeyRef.current !== ownerKeyAtUpdate) return prev;
      return updater(prev);
    });
  }, [conversationOwnerKey]);

  const toggleAgentConversationPin = useCallback((conversation: AgentConversation) => {
    const nextPinned = !conversation.pinned;
    updateAgentConversations(items => items.map(item =>
      item.sessionId === conversation.sessionId ? { ...item, pinned: nextPinned } : item
    ));
    if (isTauriRuntime()) {
      updateDesktopConversationPinned(conversation.sessionId, nextPinned).catch((err) => {
        setAgentConversationSyncError("保存本地置顶状态失败。");
        console.warn("保存本地置顶状态失败", err);
      });
      setAgentConversationMenuId(null);
      return;
    }
    fetch(`${API_BASE}/api/agent/conversations/${encodeURIComponent(conversation.sessionId)}`, {
      method: "PATCH",
      credentials: "include",
      headers: authHeaders({ "Content-Type": "application/json" }),
      body: JSON.stringify({ pinned: nextPinned }),
    }).catch((err) => {
      setAgentConversationSyncError("置顶状态同步到服务器失败。");
      console.warn("同步置顶状态失败", err);
    });
    setAgentConversationMenuId(null);
  }, [updateAgentConversations]);

  const deleteAgentConversation = useCallback((conversation: AgentConversation) => {
    updateAgentConversations(items => items.filter(item => item.sessionId !== conversation.sessionId));
    if (isTauriRuntime()) {
      archiveDesktopConversation(conversation.sessionId).catch((err) => {
        setAgentConversationSyncError("归档本地会话失败。");
        console.warn("归档本地会话失败", err);
      });
      if (conversation.sessionId === agentSessionId) {
        setAgentSessionId("");
        setAgentMessages([]);
        setAgentResponse(null);
        setAgentProgress(initialAgentProgress);
        setAgentError("");
      }
      setAgentConversationMenuId(null);
      return;
    }
    fetch(`${API_BASE}/api/agent/conversations/${encodeURIComponent(conversation.sessionId)}`, {
      method: "DELETE",
      credentials: "include",
      headers: authHeaders(),
    }).catch((err) => {
      setAgentConversationSyncError("删除会话同步到服务器失败。");
      console.warn("同步删除会话失败", err);
    });
    if (conversation.sessionId === agentSessionId) {
      setAgentSessionId("");
      setAgentMessages([]);
      setAgentResponse(null);
      setAgentProgress(initialAgentProgress);
      setAgentError("");
    }
    setAgentConversationMenuId(null);
  }, [agentSessionId, setAgentError, setAgentProgress, updateAgentConversations]);

  const removeConversationBySessionId = useCallback((sessionId: string) => {
    updateAgentConversations(items => items.filter(item => item.sessionId !== sessionId));
  }, [updateAgentConversations]);

  const beginRenameAgentConversation = useCallback((conversation: AgentConversation) => {
    setRenamingConversationId(conversation.sessionId);
    setRenamingConversationTitle(conversation.title);
    setAgentConversationMenuId(null);
  }, []);

  const commitRenameAgentConversation = useCallback(() => {
    if (!renamingConversationId) return;
    const title = renamingConversationTitle.trim();
    if (!title) {
      setRenamingConversationId(null);
      return;
    }
    updateAgentConversations(items => items.map(item =>
      item.sessionId === renamingConversationId ? { ...item, title } : item
    ));
    if (isTauriRuntime()) {
      updateDesktopConversationTitle(renamingConversationId, title).catch((err) => {
        setAgentConversationSyncError("重命名本地会话失败。");
        console.warn("重命名本地会话失败", err);
      });
      setRenamingConversationId(null);
      return;
    }
    fetch(`${API_BASE}/api/agent/conversations/${encodeURIComponent(renamingConversationId)}`, {
      method: "PATCH",
      credentials: "include",
      headers: authHeaders({ "Content-Type": "application/json" }),
      body: JSON.stringify({ title }),
    }).catch((err) => {
      setAgentConversationSyncError("重命名同步到服务器失败。");
      console.warn("同步重命名失败", err);
    });
    setRenamingConversationId(null);
  }, [renamingConversationId, renamingConversationTitle, updateAgentConversations]);

  const shareAgentConversation = useCallback(async (conversation: AgentConversation) => {
    const shareText = `${conversation.title}\n${window.location.origin}${window.location.pathname}`;
    try {
      if (navigator.share) {
        await navigator.share({ title: conversation.title, text: shareText });
      } else {
        await navigator.clipboard?.writeText(shareText);
      }
    } catch {
      // User cancelled the native share sheet; no UI interruption is needed.
    } finally {
      setAgentConversationMenuId(null);
    }
  }, []);

  const cancelEditAgentMessage = useCallback(() => {
    setEditingAgentMessageIndex(null);
    setEditingAgentMessageText("");
  }, []);

  const beginEditAgentMessage = useCallback((content: string, index: number) => {
    if (agentLoading) return;
    setEditingAgentMessageIndex(index);
    setEditingAgentMessageText(content);
  }, [agentLoading]);

  useEffect(() => {
    if (!isTauriRuntime() || !agentHistorySearchOpen) return;
    const query = agentHistorySearch.trim();
    if (!query) {
      setDesktopSearchConversations([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void searchDesktopHistory(query, undefined, false, 30)
        .then(async (hits) => {
          const seen = new Set<string>();
          const uniqueHits = hits.filter((hit) => {
            if (seen.has(hit.conversation_id)) return false;
            seen.add(hit.conversation_id);
            return true;
          });
          const conversations = await Promise.all(uniqueHits.map(loadDesktopAgentConversationFromHit));
          if (!cancelled) {
            setDesktopSearchConversations(conversations);
            setAgentConversationSyncError("");
          }
        })
        .catch((err) => {
          if (!cancelled) {
            setDesktopSearchConversations([]);
            setAgentConversationSyncError("搜索本地桌面对话失败。");
            console.warn("搜索本地桌面对话失败", err);
          }
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [agentHistorySearch, agentHistorySearchOpen]);

  const filteredAgentConversations = useMemo(() => (
    isTauriRuntime() && agentHistorySearch.trim()
      ? desktopSearchConversations
      : agentConversations
        .filter((conversation) => {
          const keyword = agentHistorySearch.trim().toLowerCase();
          if (!keyword) return true;
          return conversation.title.toLowerCase().includes(keyword)
            || conversation.messages.some((message) => message.content.toLowerCase().includes(keyword));
        })
        .sort((a, b) => Number(Boolean(b.pinned)) - Number(Boolean(a.pinned)) || new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
  ), [agentConversations, agentHistorySearch, desktopSearchConversations]);

  const recentAgentConversations = useMemo(() => (
    [...agentConversations]
      .sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
      .slice(0, 8)
  ), [agentConversations]);

  useEffect(() => {
    if (!agentHistorySearchOpen && !agentRecentChatsOpen) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setAgentHistorySearchOpen(false);
        setAgentRecentChatsOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [agentHistorySearchOpen, agentRecentChatsOpen]);

  return {
    agentSessionId,
    setAgentSessionId,
    agentMessages,
    setAgentMessages,
    agentResponse,
    setAgentResponse,
    agentConversations,
    setAgentConversations,
    agentHistorySearchOpen,
    setAgentHistorySearchOpen,
    agentHistorySearch,
    setAgentHistorySearch,
    agentRecentChatsOpen,
    setAgentRecentChatsOpen,
    agentModelMenuOpen,
    setAgentModelMenuOpen,
    agentConversationSyncError,
    setAgentConversationSyncError,
    agentConversationMenuId,
    setAgentConversationMenuId,
    renamingConversationId,
    setRenamingConversationId,
    renamingConversationTitle,
    setRenamingConversationTitle,
    copiedAgentMessageIndex,
    setCopiedAgentMessageIndex,
    editingAgentMessageIndex,
    editingAgentMessageText,
    setEditingAgentMessageText,
    agentSidebarCollapsed,
    setAgentSidebarCollapsed,
    filteredAgentConversations,
    recentAgentConversations,
    persistAgentConversation,
    startNewAgentConversation,
    selectAgentConversation,
    toggleAgentConversationPin,
    deleteAgentConversation,
    removeConversationBySessionId,
    beginRenameAgentConversation,
    commitRenameAgentConversation,
    shareAgentConversation,
    beginEditAgentMessage,
    cancelEditAgentMessage,
  };
}
