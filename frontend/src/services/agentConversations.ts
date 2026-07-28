import type { AgentConversation, AgentMessage, AgentResponse } from "../agent/types";
import { API_BASE } from "./api";

function restoreAgentMessageAction(message: AgentMessage): AgentMessage {
  if (message.role !== "agent" || message.action) return message;
  const content = message.content || "";
  const looksLikeOptimizationAdvice = content.includes("可选成分方案")
    && (content.includes("推荐方案") || content.includes("出钢记号"));
  if (!looksLikeOptimizationAdvice) return message;
  return {
    ...message,
    action: { type: "process_optimization", label: "工艺寻优" },
  };
}

export function normalizeAgentConversation(raw: any): AgentConversation | null {
  const sessionId = raw?.sessionId ?? raw?.session_id;
  const updatedAt = raw?.updatedAt ?? raw?.updated_at ?? new Date().toISOString();
  if (typeof sessionId !== "string" || typeof raw?.title !== "string" || !Array.isArray(raw?.messages)) {
    return null;
  }
  return {
    sessionId,
    title: raw.title,
    updatedAt,
    messages: raw.messages.map(restoreAgentMessageAction),
    response: raw.response ?? null,
    pinned: Boolean(raw.pinned),
  };
}

function compactAgentMessagesForRemote(messages: AgentMessage[]) {
  return messages.map(message => ({
    role: message.role,
    content: message.content,
    action: message.action,
    evidence: message.streamEvidence,
    reasoning: message.reasoning,
    reasoningMs: message.reasoningMs,
  }));
}

function compactAgentResponseForRemote(response: AgentResponse | null) {
  if (!response) return null;
  return {
    ...response,
    memory: undefined,
    workflow: response.workflow
      ? {
          ...response.workflow,
          events: [],
        }
      : response.workflow,
  };
}

export async function saveAgentConversationRemote(conversation: AgentConversation) {
  const resp = await fetch(`${API_BASE}/api/agent/conversations/${encodeURIComponent(conversation.sessionId)}`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      session_id: conversation.sessionId,
      title: conversation.title,
      messages: compactAgentMessagesForRemote(conversation.messages),
      response: compactAgentResponseForRemote(conversation.response),
      pinned: Boolean(conversation.pinned),
    }),
  });
  if (!resp.ok) throw new Error(await resp.text());
}

export function buildAgentConversationTitle(messages: AgentMessage[]) {
  const firstUserMessage = messages.find((message) => message.role === "user")?.content?.trim();
  if (!firstUserMessage) return "新对话";
  return firstUserMessage.length > 28 ? `${firstUserMessage.slice(0, 28)}...` : firstUserMessage;
}
