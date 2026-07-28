import type { AnswerReferenceResult } from "../components/answer/AnswerRenderer";

export interface QueryRequest {
  query_text: string;
  slab_width_min: number;
  slab_width_max: number;
  slab_thickness_min: number;
  slab_thickness_max: number;
  yield_rp02_min: number;
  yield_rp02_max: number;
  tensile_strength_min: number;
  tensile_strength_max: number;
  elongation_min: number;
  elongation_max: number;
  top_k: number;
  include_production: boolean;
  steel_mark: string;
  steel_grade: string;
  advice_mode: "" | "composition" | "process";
}

export interface FieldRange {
  min_val: number;
  max_val: number;
}

export interface OverviewResponse {
  literature_papers_count: number;
  literature_images_count: number;
  experimental_images_count: number;
  production_count: number;
  slab_width_range?: FieldRange | null;
  slab_thickness_range?: FieldRange | null;
  yield_rp02_range?: FieldRange | null;
  tensile_strength_range?: FieldRange | null;
  elongation_range?: FieldRange | null;
}

export interface LLMConfigInfo {
  provider: string;
  base_url: string;
  model_name: string;
  api_key_configured: boolean;
  api_key_preview: string;
}

export interface UserProfileInfo {
  username: string;
  role: string;
  edition: string;
  llm: LLMConfigInfo;
  stats: {
    literature_papers_count?: number;
    literature_images_count?: number;
    experimental_images_count?: number;
    production_count?: number;
    server_uptime_seconds?: number;
    agent_conversation_storage?: string;
  };
  traffic?: {
    total_sent_bytes: number;
    total_recv_bytes: number;
    session_sent_bytes: number;
    session_recv_bytes: number;
    interfaces: Array<{ name: string; sent_bytes: number; recv_bytes: number; is_possible_vpn?: boolean }>;
    campus_vpn_detected: boolean;
    note: string;
  };
}

export interface UserProfileStatsInfo {
  stats: UserProfileInfo["stats"];
  traffic: NonNullable<UserProfileInfo["traffic"]>;
}

export interface TurnstileAdminConfigInfo {
  enabled: boolean;
  site_key: string;
  secret_key_configured: boolean;
  secret_key_preview: string;
}

export type CaptchaProviderValue = "turnstile" | "geetest" | "slider" | "none";

export interface CaptchaAdminConfigInfo {
  provider: CaptchaProviderValue;
  turnstile_site_key: string;
  turnstile_secret_key_configured: boolean;
  turnstile_secret_key_preview: string;
  geetest_captcha_id: string;
  geetest_private_key_configured: boolean;
  geetest_private_key_preview: string;
}

export interface AuthSecurityConfigInfo {
  registration_enabled: boolean;
  email_verify_enabled: boolean;
  password_reset_enabled: boolean;
  frontend_url: string;
}

export interface KnowledgeBaseSecurityConfigInfo {
  shared_enabled: boolean;
}

export type MinerUProviderMode = "lab_only" | "cloud_only" | "cloud_first" | "lab_first";

export type RetrievalProviderMode = "lab_only" | "cloud_only";

export interface RetrievalModelsConfigInfo {
  provider_mode: RetrievalProviderMode;
  api_base: string;
  embedding_model: string;
  rerank_model: string;
  api_key_configured: boolean;
  api_key_preview: string;
}

export interface MinerUProcessingConfigInfo {
  provider_mode: MinerUProviderMode;
  api_base: string;
  model_version: string;
  batch_size: number;
  file_is_ocr: boolean;
  enable_formula: boolean;
  enable_table: boolean;
  api_key_configured: boolean;
  api_key_preview: string;
}

export interface MinerUUsageInfo {
  configured: boolean;
  provider_mode: MinerUProviderMode;
  api_base: string;
  model_version: string;
  limits: {
    max_file_size_mb?: number;
    max_pages?: number;
    max_batch_files?: number;
    supported_models?: string[];
    recommended_model?: string;
    [key: string]: unknown;
  };
  usage_available: boolean;
  usage: Record<string, unknown>;
  error: string;
}

export interface ProductionStats {
  total_batches: number;
  avg_nb_content: number;
  avg_yield_strength: number;
  avg_ti_content: number;
  avg_fast_cooling_temp: number;
}

export interface ProductionRecord {
  [key: string]: string | number | null;
}

export interface LitResult extends AnswerReferenceResult {}

export interface ImageResult {
  paper_name?: string;
  header_path?: string;
  image_path: string;
  caption?: string;
  description?: string;
  similarity_score: number;
}

export interface SearchResponse {
  success: boolean;
  production_stats: ProductionStats | null;
  production_columns: string[];
  production_records: ProductionRecord[];
  advice_mode?: "composition" | "process" | null;
  advice_prompt?: string | null;
  advice_contexts?: string[];
  advice_record_count?: number;
  advice_standard_columns?: string[];
  advice_standard_records?: (Record<string, unknown> | null)[];
  literature_results: LitResult[];
  literature_images: ImageResult[];
  experimental_images: ImageResult[];
  error?: string;
}

export type TabId = "literature" | "litImages" | "expImages" | "production" | "standard";
export type AppMode = "select" | "retrieval" | "agent";
