import { invokeDesktop } from "./tauri";

export type DesktopConversation = {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  pinned: boolean;
  archived: boolean;
};

export type DesktopMessage = {
  id: string;
  conversation_id: string;
  role: "user" | "agent" | "assistant" | "system";
  content: string;
  response_json?: string | null;
  created_at: string;
};

export type DesktopHistoryHit = {
  conversation_id: string;
  conversation_title: string;
  message_id: string;
  role: DesktopMessage["role"];
  content: string;
  created_at: string;
  score: number;
  snippet: string;
};

export type DesktopConversationSnapshotMessage = {
  role: DesktopMessage["role"];
  content: string;
  response_json?: string | null;
};

export function listConversations() {
  return invokeDesktop<DesktopConversation[]>("list_conversations");
}

export function listArchivedConversations() {
  return invokeDesktop<DesktopConversation[]>("list_archived_conversations");
}

export function createConversation(title: string) {
  return invokeDesktop<DesktopConversation>("create_conversation", { title });
}

export function updateConversationTitle(conversationId: string, title: string) {
  return invokeDesktop<void>("update_conversation_title", { conversationId, title });
}

export function updateConversationPinned(conversationId: string, pinned: boolean) {
  return invokeDesktop<void>("update_conversation_pinned", { conversationId, pinned });
}

export function archiveConversation(conversationId: string) {
  return invokeDesktop<void>("archive_conversation", { conversationId });
}

export function restoreConversation(conversationId: string) {
  return invokeDesktop<void>("restore_conversation", { conversationId });
}

export function listMessages(conversationId: string) {
  return invokeDesktop<DesktopMessage[]>("list_messages", { conversationId });
}

export function searchHistory(query: string, conversationId?: string, excludeCurrent = false, limit = 8) {
  return invokeDesktop<DesktopHistoryHit[]>("search_history", {
    query,
    conversationId,
    excludeCurrent,
    limit,
  });
}

export function appendMessage(
  conversationId: string,
  role: DesktopMessage["role"],
  content: string,
  responseJson?: string,
) {
  return invokeDesktop<DesktopMessage>("append_message", {
    conversationId,
    role,
    content,
    responseJson,
  });
}

export function saveConversationSnapshot(
  conversationId: string,
  title: string,
  messages: DesktopConversationSnapshotMessage[],
) {
  return invokeDesktop<void>("save_conversation_snapshot", {
    conversationId,
    title,
    messages,
  });
}

export function replaceMessageAfterEdit(messageId: string, content: string) {
  return invokeDesktop<void>("replace_message_after_edit", { messageId, content });
}

export function truncateConversationAfterMessage(messageId: string) {
  return invokeDesktop<void>("truncate_conversation_after_message", { messageId });
}

export function forkConversationFromMessage(messageId: string) {
  return invokeDesktop<DesktopConversation>("fork_conversation_from_message", { messageId });
}
