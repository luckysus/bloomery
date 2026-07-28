import { invokeDesktop } from "./tauri";

export type DesktopContextBudgetMeta = {
  recent_message_token_budget?: number;
  recent_message_count?: number;
  selected_memory_count?: number;
  memory_index_count?: number;
  memory_index_total_count?: number;
  memory_index_truncated?: boolean;
  memory_index_tokens?: number;
  history_hit_count?: number;
  estimated_context_tokens?: number;
  summary_tokens?: number;
};

export type DesktopContextMessage = {
  role?: string;
  content?: string;
  created_at?: string;
};

export type DesktopContextMemory = {
  id?: string;
  scope?: string;
  type?: string;
  title?: string;
  description?: string;
  body?: string;
  tags_json?: string;
  score?: number;
  snippet?: string;
  updated_at?: string;
};

export type DesktopContextHistoryHit = {
  conversation_id?: string;
  conversation_title?: string;
  message_id?: string;
  role?: string;
  content?: string;
  created_at?: string;
  score?: number;
  snippet?: string;
};

export type DesktopContextPacket = {
  conversation_summary: string;
  recent_messages: DesktopContextMessage[];
  memory_index: DesktopContextMemory[];
  selected_memories: DesktopContextMemory[];
  history_hits: DesktopContextHistoryHit[];
  desktop_meta: {
    client?: string;
    context_version?: number;
    conversation_id?: string;
    query_length?: number;
    budget_meta?: DesktopContextBudgetMeta;
    [key: string]: unknown;
  };
};

export function buildContextPacket(conversationId: string, message: string) {
  return invokeDesktop<DesktopContextPacket>("build_context_packet", { conversationId, message });
}
