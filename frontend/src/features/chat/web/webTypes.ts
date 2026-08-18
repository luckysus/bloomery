import type { EvidenceItem, Message } from "../../../bridge/desktop";

export interface WebRecommendation {
  title: string;
  category?: string;
  summary?: string;
  details?: Record<string, unknown>;
  risks?: string[];
}

export interface WebPendingConfirmation {
  action_id: string;
  tool_name: string;
  title: string;
  permission: string;
  arguments: Record<string, unknown>;
  warning?: string;
}

export interface WebSource {
  index: number;
  title: string;
  url: string;
  site?: string;
  date?: string;
  snippet?: string;
}

export interface WebResponse {
  follow_up_questions: string[];
  recommendations: WebRecommendation[];
  pending_confirmations: WebPendingConfirmation[];
  evidence: EvidenceItem[];
  reasoning?: string;
  reasoning_ms?: number;
  web_sources: WebSource[];
}

export interface WebMessage {
  role: "user" | "agent";
  content: string;
  response: WebResponse | null;
  streamEvidence: EvidenceItem[];
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" ? value as Record<string, unknown> : null;
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function asEvidence(value: unknown): EvidenceItem[] {
  return Array.isArray(value) ? value.filter((item): item is EvidenceItem => {
    const record = asRecord(item);
    return Boolean(record && typeof record.citation_number === "number" && record.chunk);
  }) : [];
}

function asRecommendations(value: unknown): WebRecommendation[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    const record = asRecord(item);
    if (!record || typeof record.title !== "string") return [];
    return [{
      title: record.title,
      category: typeof record.category === "string" ? record.category : undefined,
      summary: typeof record.summary === "string" ? record.summary : undefined,
      details: asRecord(record.details) ?? undefined,
      risks: asStringArray(record.risks),
    }];
  });
}

function asConfirmations(value: unknown): WebPendingConfirmation[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    const record = asRecord(item);
    if (!record || typeof record.action_id !== "string" || typeof record.title !== "string") return [];
    return [{
      action_id: record.action_id,
      tool_name: typeof record.tool_name === "string" ? record.tool_name : "",
      title: record.title,
      permission: typeof record.permission === "string" ? record.permission : "confirm",
      arguments: asRecord(record.arguments) ?? {},
      warning: typeof record.warning === "string" ? record.warning : undefined,
    }];
  });
}

function asWebSources(value: unknown): WebSource[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item, index) => {
    const record = asRecord(item);
    if (!record || typeof record.url !== "string" || typeof record.title !== "string") return [];
    return [{
      index: typeof record.index === "number" ? record.index : index + 1,
      title: record.title,
      url: record.url,
      site: typeof record.site === "string" ? record.site : undefined,
      date: typeof record.date === "string" ? record.date : undefined,
      snippet: typeof record.snippet === "string" ? record.snippet : undefined,
    }];
  });
}

export function parseWebResponse(message: Message): WebResponse | null {
  if (!["agent", "assistant"].includes(message.role) || !message.response_json) return null;
  try {
    const record = asRecord(JSON.parse(message.response_json));
    if (!record) return null;
    return {
      follow_up_questions: asStringArray(record.follow_up_questions),
      recommendations: asRecommendations(record.recommendations),
      pending_confirmations: asConfirmations(record.pending_confirmations),
      evidence: asEvidence(record.evidence),
      reasoning: typeof record.reasoning === "string" ? record.reasoning : undefined,
      reasoning_ms: typeof record.reasoning_ms === "number" ? record.reasoning_ms : undefined,
      web_sources: asWebSources(record.web_sources),
    };
  } catch {
    return null;
  }
}

export function toWebMessage(message: Message): WebMessage {
  return {
    role: message.role === "user" ? "user" : "agent",
    content: message.content,
    response: parseWebResponse(message),
    streamEvidence: parseWebResponse(message)?.evidence ?? [],
  };
}
