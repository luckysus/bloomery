import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  open as openNativeDialog,
  save as saveNativeDialog,
  type OpenDialogOptions,
  type SaveDialogOptions,
} from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import type { AgentEventEnvelope, AgentRunState, PermissionDecision } from "./generated/protocol";

export interface UpdateInfo {
  version: string;
  date: string | null;
  body: string | null;
}

let pendingUpdate: Awaited<ReturnType<typeof check>> = null;

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

export interface PermissionRuleRecord {
  id: string;
  tool_id: string;
  tool_version: { major: number; minor: number; patch: number };
  source: {
    kind: "builtin";
    } | {
    kind: "mcp";
    server_id: string;
    server_version: { major: number; minor: number; patch: number };
  } | {
    kind: "domain";
    package_id: string;
    package_version: { major: number; minor: number; patch: number };
  };
  action: "execute";
  scope:
    | { kind: "any" }
    | { kind: "exact"; value: unknown }
    | { kind: "fields"; value: Record<string, unknown> };
  effect: "allow" | "deny";
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

export interface DatasetColumnMapping {
  ordinal: number;
  canonicalField?: string | null;
  unit?: string | null;
}

export interface SteelDatasetColumnRecord {
  ordinal: number;
  originalName: string;
  duplicate: boolean;
  inferredType: "number" | "text" | "date";
  canonicalField: string | null;
  unit: string | null;
  nonEmptyCount: number;
  missingCount: number;
  invalidCount: number;
  min: number | null;
  max: number | null;
}

export interface SteelDatasetRecord {
  id: string;
  sourceName: string;
  sourcePath: string;
  sourceSha256: string;
  format: string;
  selectedSheet: string;
  rowCount: number;
  columnCount: number;
  truncated: boolean;
  mappingState: "draft" | "ready";
  preview: DatasetPreview;
  columns: SteelDatasetColumnRecord[];
  createdAt: string;
  updatedAt: string;
}

export interface DatasetValueFrequency {
  value: string;
  count: number;
}

export interface DatasetDistributionBin {
  lowerBound: number;
  upperBound: number;
  count: number;
}

export interface DatasetColumnAnalysis {
  ordinal: number;
  name: string;
  canonicalField: string | null;
  unit: string | null;
  inferredType: "number" | "text";
  sampleCount: number;
  missingCount: number;
  invalidCount: number;
  missingRate: number;
  distinctCount: number;
  mean: number | null;
  standardDeviation: number | null;
  min: number | null;
  percentile25: number | null;
  median: number | null;
  percentile75: number | null;
  max: number | null;
  outlierCount: number;
  outlierRows: number[];
  topValues: DatasetValueFrequency[];
  distribution: DatasetDistributionBin[];
}

export interface DatasetGroupColumnSummary {
  ordinal: number;
  sampleCount: number;
  mean: number | null;
  min: number | null;
  max: number | null;
}

export interface DatasetGroupSummary {
  key: string;
  rowCount: number;
  columns: DatasetGroupColumnSummary[];
}

export interface DatasetCorrelation {
  leftOrdinal: number;
  rightOrdinal: number;
  sampleCount: number;
  pearson: number | null;
}

export interface DatasetAnalysis {
  datasetId: string | null;
  sourceSha256: string | null;
  selectedSheet: string | null;
  rowCount: number;
  analyzedRowCount: number;
  excludedRowCount: number;
  columns: DatasetColumnAnalysis[];
  groups: DatasetGroupSummary[];
  correlations: DatasetCorrelation[];
  warnings: string[];
}

export interface TrainSteelDatasetRequest {
  datasetId: string;
  targetColumn: number;
  featureColumns: number[];
  splitPolicy?: {
    kind: "random" | "group" | "time";
    validationFraction: number;
    seed?: number;
  };
}

export interface ComputeTrainingResult {
  task_id: string;
  state: "completed";
  artifact: {
    model_id: string;
    model_type: string;
    feature_names: string[];
    metrics: Record<string, unknown>;
    applicability_range: Array<{ min: number | null; max: number | null }>;
  };
}

export interface ComputePredictionResult {
  task_id: string;
  state: "completed";
  model_id: string;
  model_type: string;
  feature_names: string[];
  input_values: Array<number | null>;
  predictions: number[];
  applicability_range: Array<{ min: number | null; max: number | null }>;
  applicability_warnings: Array<{
    code: string;
    feature: string;
    index: number;
    value: number;
    min: number | null;
    max: number | null;
  }>;
  confidence: number | null;
  constraints: unknown[];
}

export interface ComputeOptimizationRequest {
  datasetId: string;
  trainingTaskId: string;
  direction: "minimize" | "maximize";
  objectiveColumns: number[];
  bounds: Array<{ min: number; max: number }>;
  fixedValues: Array<number | null>;
  constraints: Array<{
    kind: "equality" | "inequality";
    coefficients: number[];
    value: number;
    tolerance?: number;
  }>;
  trials: number;
  seed: number;
}

export interface ComputeOptimizationRecommendation {
  values: Record<string, number>;
  objectives: number[];
  prediction: number;
  feasible: boolean;
  constraint_residuals: Record<string, number>;
}

export interface ComputeOptimizationResult {
  task_id: string;
  state: "completed";
  method: string;
  direction: "minimize" | "maximize";
  objectives: string[];
  feature_names: string[];
  model_id: string;
  model_type: string;
  trials_completed: number;
  deterministic_seed: number;
  recommendations: ComputeOptimizationRecommendation[];
}

export interface ComputeOnnxPredictionResult {
  task_id: string;
  state: "completed";
  model_id: string;
  model_version: string;
  model_sha256: string;
  opset_version: number;
  operators: string[];
  input_schema: Array<{ name: string; type: string; shape: Array<number | null> }>;
  output_schema: Array<{ name: string; type: string; shape: Array<number | null> }>;
  preprocessing: { feature_names: string[]; means: number[]; scales: number[] };
  normalized_inputs: number[][];
  predictions: number[] | number[][];
  outputs: Record<string, unknown>;
  applicability_warnings: Array<{
    row: number;
    feature: string;
    index: number;
    value: number;
    min: number | null;
    max: number | null;
    code: string;
  }>;
  confidence: number[] | null;
  constraints: unknown[];
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
  dataset_id?: string | null;
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

export interface BackupSummary {
  format_version: number;
  archive_path: string;
  database_bytes: number;
  content_file_count: number;
  content_bytes: number;
}

export type ConversationExportFormat = "markdown" | "json";

export interface ConversationExportSummary {
  format: ConversationExportFormat;
  output_path: string;
  message_count: number;
  bytes: number;
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

export interface DomainInstallResult {
  package: DomainPackageRecord;
  replaced_active_version: string | null;
}

export interface DomainPackageImpact {
  package_id: string;
  version: string;
  active: boolean;
  tool_count: number;
  mcp_recommendation_count: number;
  asset_count: number;
}

export type McpTransportKind = "stdio" | "streamable_http" | "sse";

export interface McpServerSummary {
  id: string;
  display_name: string;
  server_id: string;
  transport: McpTransportKind;
  url: string | null;
  executable: string | null;
  args: string[];
  working_directory: string | null;
  env_names: string[];
  timeout_ms: number;
  enabled: boolean;
  secret_configured: boolean;
  status: string;
  last_error: string | null;
  last_checked_at: string | null;
  tool_count: number;
}

export interface McpServerInput {
  id?: string | null;
  display_name: string;
  server_id: string;
  transport: McpTransportKind;
  url: string | null;
  executable: string | null;
  args: string[];
  working_directory: string | null;
  inherited_env: string[];
  env_values: Record<string, string>;
  bearer_token?: string;
  clear_bearer_token?: boolean;
  timeout_ms: number;
  enabled: boolean;
}

export interface McpToolSummary {
  id: string;
  name: string;
  description: string;
}

export interface McpHealth {
  status: "healthy" | "failed";
  server_name: string | null;
  server_version: string | null;
  tool_count: number;
  resource_count: number;
  prompt_count: number;
  tools: McpToolSummary[];
  error: string | null;
  checked_at: string;
}

export type FileDialogOptions = OpenDialogOptions;
export type SaveFileDialogOptions = SaveDialogOptions;

function call<T>(command: string, args?: Record<string, unknown>) {
  if (!isDesktopRuntime()) return Promise.reject(new Error("Desktop runtime is unavailable"));
  return invoke<T>(command, args);
}

export const desktop = {
  async initialize() {
    if (!isDesktopRuntime()) return;
    await invoke<void>("db_init");
  },
  checkForUpdate: async (): Promise<UpdateInfo | null> => {
    if (!isDesktopRuntime()) return null;
    pendingUpdate = await check();
    if (!pendingUpdate) return null;
    return {
      version: pendingUpdate.version,
      date: pendingUpdate.date ?? null,
      body: pendingUpdate.body ?? null,
    };
  },
  installUpdate: async () => {
    if (!isDesktopRuntime()) return;
    if (!pendingUpdate) throw new Error("No update is ready to install");
    await pendingUpdate.downloadAndInstall();
    await relaunch();
  },
  getSetting: (key: string) => call<string | null>("get_setting", { key }),
  openFileDialog: (options?: FileDialogOptions) => openNativeDialog(options),
  saveFileDialog: (options?: SaveFileDialogOptions) => saveNativeDialog(options),
  setSetting: (key: string, valueJson: string) =>
    call<void>("set_setting", { key, valueJson }),
  listConversations: () => call<Conversation[]>("list_conversations"),
  createConversation: (title: string) =>
    call<Conversation>("create_conversation", { title }),
  listMessages: (conversationId: string) =>
    call<Message[]>("list_messages", { conversationId }),
  exportConversation: (conversationId: string, outputPath: string, format: ConversationExportFormat) =>
    call<ConversationExportSummary>("export_conversation", { conversationId, outputPath, format }),
  getConversationDraft: (conversationId: string) =>
    call<string>("get_conversation_draft", { conversationId }),
  saveConversationDraft: (conversationId: string, content: string) =>
    call<void>("save_conversation_draft", { conversationId, content }),
  desktopAgentChat: (request: LocalAgentChatRequest) =>
    call<LocalAgentChatResponse>("desktop_agent_chat", { request }),
  resolveAgentPermission: (permissionId: string, decision: PermissionDecision) =>
    call<void>("resolve_agent_permission", { permissionId, decision }),
  listPermissionRules: () => call<PermissionRuleRecord[]>("list_permission_rules"),
  revokePermissionRule: (ruleId: string) =>
    call<void>("revoke_permission_rule", { ruleId }),
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
  listSteelDatasets: () => call<SteelDatasetRecord[]>("list_steel_datasets"),
  saveSteelDataset: (request: {
    sourcePath: string;
    sheet?: string;
    mappings?: DatasetColumnMapping[];
  }) => call<SteelDatasetRecord>("save_steel_dataset", { request }),
  activateSteelDataset: (datasetId: string) =>
    call<SteelDatasetRecord>("activate_steel_dataset", { datasetId }),
  analyzeSteelDataset: (request: {
    datasetId: string;
    selectedColumns?: number[];
    outlierIqrMultiplier?: number;
    groupByColumn?: number;
    correlationColumns?: number[];
  }) => call<DatasetAnalysis>("analyze_steel_dataset", { request }),
  trainSteelDataset: (request: TrainSteelDatasetRequest) =>
    call<BackgroundTask>("train_steel_dataset", { request }),
  getComputeTrainingResult: (id: string) =>
    call<ComputeTrainingResult | null>("get_compute_training_result", { id }),
  predictSteelModel: (request: {
    datasetId: string;
    trainingTaskId: string;
    featureValues: number[];
  }) => call<BackgroundTask>("predict_steel_model", { request }),
  getComputePredictionResult: (id: string) =>
    call<ComputePredictionResult | null>("get_compute_prediction_result", { id }),
  hashOnnxModelFile: (path: string) =>
    call<string>("hash_onnx_model_file", { path }),
optimizeSteelProcess: (request: ComputeOptimizationRequest) =>
    call<BackgroundTask>("optimize_steel_process", { request }),
getComputeOptimizationResult: (id: string) =>
    call<ComputeOptimizationResult | null>("get_compute_optimization_result", { id }),
  predictOnnxModel: (request: {
    modelPath: string;
    modelSha256: string;
    manifest: Record<string, unknown>;
    features: number[][];
  }) => call<BackgroundTask>("predict_onnx_model", { request }),
  getComputeOnnxPredictionResult: (id: string) =>
    call<ComputeOnnxPredictionResult | null>("get_compute_onnx_prediction_result", { id }),
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
    call<DomainInstallResult>("install_domain_package", { sourcePath }),
  installBundledSteelPackage: () =>
    call<DomainInstallResult>("install_bundled_steel_package"),
  activateDomainPackage: (packageId: string, version: string) =>
    call<DomainPackageRecord>("activate_domain_package", { packageId, version }),
  previewRemoveDomainPackage: (packageId: string, version: string) =>
    call<DomainPackageImpact>("preview_remove_domain_package", { packageId, version }),
  removeDomainPackage: (packageId: string, version: string) =>
    call<void>("remove_domain_package", { packageId, version }),
  listMcpServers: () => call<McpServerSummary[]>("list_mcp_servers"),
  saveMcpServer: (server: McpServerInput) =>
    call<McpServerSummary>("save_mcp_server", { input: server }),
  checkMcpServer: (id: string) => call<McpHealth>("check_mcp_server", { id }),
  restartMcpServer: (id: string) => call<McpHealth>("restart_mcp_server", { id }),
  listMcpTools: (id: string) => call<McpToolSummary[]>("list_mcp_tools", { id }),
  deleteMcpServer: (id: string) => call<void>("delete_mcp_server", { id }),
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
  writeDiagnosticsExport: (outputPath: string, lastErrorKind?: string) =>
    call<void>("write_diagnostics_export", { outputPath, lastErrorKind }),
  createBackup: (archivePath: string) =>
    call<BackupSummary>("create_backup_archive", { archivePath }),
  previewBackup: (archivePath: string) =>
    call<BackupSummary>("preview_backup_archive", { archivePath }),
  restoreBackup: (archivePath: string) =>
    call<BackupSummary>("restore_backup_archive", { archivePath }),
};
