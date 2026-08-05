import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Conversation {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  pinned: boolean;
  archived: boolean;
}

export interface Message {
  id: string;
  conversation_id: string;
  role: "user" | "agent" | "assistant" | "system";
  content: string;
  response_json: string | null;
  created_at: string;
}

export interface LocalAgentChatRequest {
  sessionId?: string;
  message: string;
  runId?: string;
}

export interface LocalAgentDelta {
  run_id: string;
  delta: string;
}

export interface LocalAgentChatResponse {
  run_id: string;
  session_id: string;
  status: string;
  answer: string;
  intent?: {
    intent_type?: string;
    unavailable_capability?: string | null;
  };
}

export function isDesktopRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export interface KnowledgeBaseRecord {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface SourceDocumentRecord {
  id: string;
  knowledge_base_id: string;
  display_name: string;
  source_kind: string;
  active_version_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface DocumentVersionRecord {
  id: string;
  document_id: string;
  content_sha256: string;
  mime_type: string;
  parser: string;
  parser_version: string;
  chunk_policy_version: string;
  embedding_profile_id: string;
  embedding_model_id: string;
  embedding_dimension: number;
  expected_asset_count: number;
  expected_chunk_count: number;
  manifest_sealed: boolean;
  created_at: string;
  activated_at: string | null;
}

export interface KnowledgeBaseDeleteImpact {
  knowledge_base_id: string;
  name: string;
  document_count: number;
  version_count: number;
  chunk_count: number;
  asset_count: number;
  active_task_count: number;
}

export interface KnowledgeHealth {
  knowledge_base_count: number;
  document_count: number;
  active_document_count: number;
  version_count: number;
  chunk_count: number;
  indexed_chunk_count: number;
  active_task_count: number;
}

export interface BackgroundTask {
  id: string;
  kind: string;
  state: "queued" | "running" | "waiting_external" | "paused" | "completed" | "failed" | "cancelled" | "interrupted";
  progress: number;
  attempt: number;
  error_code: string | null;
  cancel_requested: boolean;
  can_cancel: boolean;
  can_retry: boolean;
  created_at: string;
  updated_at: string;
}

export type SourceLocation =
  | { kind: "pdf_page"; page: number; bbox: { x: number; y: number; width: number; height: number } | null }
  | { kind: "sheet_range"; sheet: string; range: string }
  | { kind: "heading"; path: string[] }
  | { kind: "text_offsets"; start: number; end: number };

export interface EvidenceChunk {
  knowledge_base_id: string;
  document_id: string;
  version_id: string;
  chunk_id: string;
  source_name: string;
  source_location: SourceLocation;
  text: string;
  lexical_rank: number | null;
  dense_rank: number | null;
  rrf_score: number;
  rerank_score: number | null;
}

export interface EvidenceAsset {
  id: string;
  kind: string;
  storage_key: string;
  media_type: string;
  source_location: SourceLocation | null;
}

export interface EvidenceItem {
  citation_number: number;
  chunk: EvidenceChunk;
  assets: EvidenceAsset[];
}

export interface EvidencePack {
  id: string;
  workspace_id: string;
  query: string;
  configuration: {
    knowledge_base_ids: string[];
    lexical_limit: number;
    dense_limit: number;
    candidate_limit: number;
    rrf_k: number;
    embedding_provider_profile_id: string;
    embedding_model_id: string;
    rerank_provider_profile_id: string | null;
    rerank_model_id: string | null;
    rerank_degradation: string | null;
  };
  evidence: EvidenceItem[];
  created_at: string;
}

export interface ResolvedCitation {
  audit_id: string;
  citation_number: number;
  label: string;
  source_state: "active" | "inactive" | "deleted";
  chunk: EvidenceChunk;
  assets: EvidenceAsset[];
}

export interface LocalKnowledgeQueryRequest {
  query: string;
  knowledge_base_ids: string[];
  lexical_limit?: number;
  dense_limit?: number;
  candidate_limit?: number;
  rrf_k?: number;
  rerank_limit?: number;
}

export interface IndexRebuildRequest {
  provider_profile_id: string;
  model_id: string;
  dimension: number;
}

export interface IndexHealthReport {
  state: "healthy" | "degraded_flat" | "rebuild_required" | "rebuilding" | "failed";
  reason:
    | "missing_sidecar"
    | "corrupt_sidecar"
    | "watermark_diverged"
    | "model_changed"
    | "interrupted_build"
    | "low_disk"
    | "rebuild_failed"
    | "sqlite_invalid"
    | null;
  serving_mode: "hnsw" | "flat" | "unavailable";
  chunk_count: number;
  required_rebuild_bytes: number;
  available_disk_bytes: number | null;
  stale_temporary_count: number;
  rebuild_task_id: string | null;
}

export interface DocumentImportRequest {
  source_path: string;
  knowledge_base: { mode: "existing"; id: string } | { mode: "create"; name: string };
  mineru_profile_id: string;
  embedding_profile_id: string;
  embedding_dimension: number;
}

export interface DocumentImportResponse {
  knowledge_base_id: string;
  document_id: string;
  version_id: string;
  ingest_attempt_id: string;
  task_id: string;
  duplicate_content: boolean;
}

export type ProviderKind = "open_ai_compatible" | "ollama" | "siliconflow" | "mineru";

export interface ProviderProfileInput {
  id?: string;
  kind: ProviderKind;
  display_name: string;
  base_url: string;
  model_id?: string | null;
  credential_name?: string | null;
  enabled: boolean;
}

export interface ProviderProfileResponse {
  id: string;
  kind: ProviderKind;
  display_name: string;
  base_url: string;
  model_id: string | null;
  enabled: boolean;
  revision: number;
  secret_generation: number;
  secret_configured: boolean;
}

export interface ProviderProbeResponse {
  ok: boolean;
  status_code: number | null;
  error_code: string | null;
  elapsed_ms: number;
}

export interface SecretStatus {
  configured: boolean;
}

function call<T>(command: string, args?: Record<string, unknown>) {
  if (!isDesktopRuntime()) return Promise.reject(new Error("Desktop runtime is unavailable"));
  return invoke<T>(command, args);
}

export const desktop = {
  async initialize() {
    if (!isDesktopRuntime()) return;
    await invoke<void>("db_init");
  },
  getSetting: (key: string) => call<string | null>("get_setting", { key }),
  setSetting: (key: string, valueJson: string) =>
    call<void>("set_setting", { key, valueJson }),
  listConversations: () => call<Conversation[]>("list_conversations"),
  createConversation: (title: string) =>
    call<Conversation>("create_conversation", { title }),
  listMessages: (conversationId: string) =>
    call<Message[]>("list_messages", { conversationId }),
  getConversationDraft: (conversationId: string) =>
    call<string>("get_conversation_draft", { conversationId }),
  saveConversationDraft: (conversationId: string, content: string) =>
    call<void>("save_conversation_draft", { conversationId, content }),
  desktopAgentChat: (request: LocalAgentChatRequest) =>
    call<LocalAgentChatResponse>("desktop_agent_chat", { request }),
  cancelDesktopRun: (runId: string) =>
    call<void>("desktop_cancel_llm_run", { runId }),
  listenDesktopAgentDeltas: (handler: (delta: LocalAgentDelta) => void) => {
    if (!isDesktopRuntime()) return Promise.resolve(() => undefined);
    return listen<LocalAgentDelta>("desktop-agent-delta", (event) => handler(event.payload));
  },
  listProviderProfiles: () => call<ProviderProfileResponse[]>("list_provider_profiles"),
  saveProviderProfile: (profile: ProviderProfileInput) =>
    call<ProviderProfileResponse>("save_provider_profile", { profile }),
  testProviderProfile: (id: string) =>
    call<ProviderProbeResponse>("test_provider_profile", { id }),
  setProviderSecret: (profileId: string, credentialName: string, value: string) =>
    call<SecretStatus>("secret_set", { profileId, credentialName, value }),
  deleteProviderSecret: (profileId: string, credentialName: string) =>
    call<SecretStatus>("secret_delete", { profileId, credentialName }),
  listKnowledgeBases: () => call<KnowledgeBaseRecord[]>("list_knowledge_bases"),
  createKnowledgeBase: (name: string) =>
    call<KnowledgeBaseRecord>("create_knowledge_base", { name }),
  renameKnowledgeBase: (id: string, name: string) =>
    call<KnowledgeBaseRecord>("rename_knowledge_base", { id, name }),
  previewDeleteKnowledgeBase: (id: string) =>
    call<KnowledgeBaseDeleteImpact>("preview_delete_knowledge_base", { id }),
  deleteKnowledgeBaseConfirmed: (id: string) =>
    call<void>("delete_knowledge_base_confirmed", { id }),
  listKnowledgeDocuments: (knowledgeBaseId: string) =>
    call<SourceDocumentRecord[]>("list_knowledge_documents", { knowledgeBaseId }),
  listDocumentVersions: (documentId: string) =>
    call<DocumentVersionRecord[]>("list_document_versions", { documentId }),
  importLocalDocument: (request: DocumentImportRequest) =>
    call<DocumentImportResponse>("import_local_document", { request }),
  listBackgroundTasks: () => call<BackgroundTask[]>("list_background_tasks"),
  cancelBackgroundTask: (id: string) =>
    call<BackgroundTask>("cancel_background_task", { id }),
  retryBackgroundTask: (id: string) =>
    call<BackgroundTask>("retry_background_task", { id }),
  rebuildKnowledgeIndex: (request: IndexRebuildRequest) =>
    call<string>("rebuild_knowledge_index", { request }),
  getIndexHealth: (request: IndexRebuildRequest) =>
    call<IndexHealthReport>("get_index_health", { request }),
  queryLocalKnowledge: (request: LocalKnowledgeQueryRequest) =>
    call<EvidencePack>("query_local_knowledge", { request }),
  resolveKnowledgeCitation: (auditId: string, citationNumber: number) =>
    call<ResolvedCitation | null>("resolve_knowledge_citation", { auditId, citationNumber }),
  getKnowledgeHealth: () => call<KnowledgeHealth>("get_knowledge_health"),
};
