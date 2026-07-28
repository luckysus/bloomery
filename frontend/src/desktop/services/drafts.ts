import { invokeDesktop } from "./tauri";

export function getConversationDraft(conversationId: string) {
  return invokeDesktop<string>("get_conversation_draft", { conversationId });
}

export function saveConversationDraft(conversationId: string, content: string) {
  return invokeDesktop<void>("save_conversation_draft", { conversationId, content });
}

export function clearConversationDraft(conversationId: string) {
  return invokeDesktop<void>("clear_conversation_draft", { conversationId });
}
