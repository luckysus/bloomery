import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AgentEventEnvelope, AgentRunState } from "./generated/protocol";

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
  evidencePackId?: string;
}

export interface LocalAgentDelta {
  run_id: string;
  delta: string;
}

export interface AgentRunRecord {
  id: string;
  workspace_id: string;
  conversation_id: string;
  user_message_id: string;
  state: AgentRunState;
  next_sequence: number;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface RunCommandResult {
  run: AgentRunRecord;
  events: AgentEventEnvelope[];
  replay_only: boolean;
}

export type RecoveryAction =
  | { kind: "regenerate" }
  | { kind: "await_permissions"; data: unknown[] }
  | { kind: "resume_tools"; data: unknown[] };

export interface RecoveredRun {
  run: AgentRunRecord;
  action: RecoveryAction;
  events: AgentEventEnvelope[];
}

export interface RunWithEvent {
  run: AgentRunRecord;
  event: AgentEventEnvelope;
}

export interface LocalAgentChatResponse {
  run_id: string;
  session_id: string;
  status: string;
  answer: string;
  evidence_pack_id?: string;
  evidence?: EvidenceItem[];
  intent?: {
    intent_type?: string;
    unavailable_capability?: string | null;
  };
}

export type CarbonEquivalentFormula = "iiw" | "pcm";
export type CompositionUnit = "percent_mass" | "mass_fraction";

export interface CarbonEquivalentResult {
  formula_id: string;
  expression: string;
  normalized_inputs: Record<string, number>;
  value: number;
  unit: CompositionUnit;
  applicability_note: string;
}

export interface DatasetPreviewColumn {
  name: string;
  duplicate: boolean;
  inferredType: "number" | "text" | "date";
  nonEmptyCount: number;
  missingCount: number;
  invalidCount: number;
  min: number | null;
  max: number | null;
}

export interface DatasetPreview {
  sourceName: string;
  format: string;
  sheets: string[];
  selectedSheet: string;
  rowCount: number;
  columnCount: number;
  truncated: boolean;
  columns: DatasetPreviewColumn[];
  sampleRows: string[][];
  warnings: string[];
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

export interface StorageHealth {
  database_ok: boolean;
  current_migration_version: number;
  latest_migration_version: number;
  database_size_bytes: number;
  reclaimable_bytes: number;
  available_disk_bytes: number | null;
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

export type SkillScope = "user" | "workspace" | "domain";

export interface SkillSource {
  scope: SkillScope;
  path: string;
}

export interface SkillSummary {
  name: string;
  description: string;
  version: string;
  compatibility: string[];
  source: SkillSource;
  content_sha256: string;
  enabled: boolean;
}

export interface SkillLoadError {
  code: string;
  path: string;
  message: string;
}

export interface SkillCatalog {
  skills: SkillSummary[];
  errors: SkillLoadError[];
}

export type DomainTrust = "official_signed" | "third_party_unsigned";

export interface DomainManifestSummary {
  id: string;
  version: string;
  author: string;
  license: string;
  builtin_tool_allowlist: string[];
  mcp_recommendations: Array<{ id: string; transport: string; description: string }>;
  assets: Array<{ path: string; kind: string; sha256: string | null }>;
}

export interface DomainPackageRecord {
  id: string;
  version: string;
  path: string;
  package_sha256: string;
  trust: DomainTrust;
  manifest: DomainManifestSummary;
  installed_at: string;
  active: boolean;
}

export interface DomainPackageImpact {
  package_id: string;
  version: string;
  active: boolean;
  tool_count: number;
  mcp_recommendation_count: number;
  asset_count: number;
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
  listenAgentEvents: (handler: (event: AgentEventEnvelope) => void) => {
    if (!isDesktopRuntime()) return Promise.resolve(() => undefined);
    return listen<AgentEventEnvelope>("agent-event", (event) => handler(event.payload));
  },
  replayAgentRun: (runId: string, afterSequence = 0) =>
    call<AgentEventEnvelope[]>("replay_agent_run", {
      request: { runId, afterSequence },
    }),
  cancelAgentRun: (runId: string, assistantMessageId?: string) =>
    call<RunCommandResult>("cancel_agent_run", {
      runId,
      assistantMessageId,
    }),
  retryAgentRun: (sourceRunId: string, runId: string, eventId?: string) =>
    call<RunWithEvent>("retry_agent_run", { sourceRunId, runId, eventId }),
  recoverAgentRuns: () => call<RecoveredRun[]>("recover_agent_runs"),
  calculateSteelCarbonEquivalent: (request: {
    formula: CarbonEquivalentFormula;
    unit: CompositionUnit;
    composition: Record<string, number>;
  }) => call<CarbonEquivalentResult>("calculate_steel_carbon_equivalent", { request }),
  previewSteelDataset: (request: { sourcePath: string; sheet?: string }) =>
    call<DatasetPreview>("preview_steel_dataset", { request }),
  listProviderProfiles: () => call<ProviderProfileResponse[]>("list_provider_profiles"),
  saveProviderProfile: (profile: ProviderProfileInput) =>
    call<ProviderProfileResponse>("save_provider_profile", { profile }),
  testProviderProfile: (id: string) =>
    call<ProviderProbeResponse>("test_provider_profile", { id }),
  setProviderSecret: (profileId: string, credentialName: string, value: string) =>
    call<SecretStatus>("secret_set", { profileId, credentialName, value }),
  deleteProviderSecret: (profileId: string, credentialName: string) =>
    call<SecretStatus>("secret_delete", { profileId, credentialName }),
  deleteProviderProfile: (id: string) => call<void>("delete_provider_profile", { id }),
  listSkills: () => call<SkillCatalog>("list_skills"),
  setSkillEnabled: (name: string, enabled: boolean) =>
    call<SkillCatalog>("set_skill_enabled", { name, enabled }),
  listDomainPackages: () => call<DomainPackageRecord[]>("list_domain_packages"),
  installDomainPackage: (sourcePath: string) =>
    call<{ package: DomainPackageRecord; replaced_active_version: string | null }>(
      "install_domain_package",
      { sourcePath },
    ),
  activateDomainPackage: (packageId: string, version: string) =>
    call<DomainPackageRecord>("activate_domain_package", { packageId, version }),
  previewRemoveDomainPackage: (packageId: string, version: string) =>
    call<DomainPackageImpact>("preview_remove_domain_package", { packageId, version }),
  removeDomainPackage: (packageId: string, version: string) =>
    call<void>("remove_domain_package", { packageId, version }),
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
  getStorageHealth: () => call<StorageHealth>("get_storage_health"),
  exportDiagnostics: (lastErrorKind?: string) =>
    call<Record<string, unknown>>("export_diagnostics", lastErrorKind ? { lastErrorKind } : undefined),
};
