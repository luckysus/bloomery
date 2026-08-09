import type { IndexRebuildRequest, KnowledgeHealth } from "../../bridge/desktop";

export const emptyHealth: KnowledgeHealth = {
  knowledge_base_count: 0,
  document_count: 0,
  active_document_count: 0,
  version_count: 0,
  chunk_count: 0,
  indexed_chunk_count: 0,
  active_task_count: 0,
};

export interface RetrievalSetup {
  embeddingProfileId: string | null;
  mineruProfileId: string | null;
}

export const emptyRetrieval: RetrievalSetup = {
  embeddingProfileId: null,
  mineruProfileId: null,
};

export const defaultEmbeddingModel = "BAAI/bge-m3";
export const defaultEmbeddingDimension = 1024;

export function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

export function parseRetrievalSetup(value: string | null): RetrievalSetup {
  if (!value) return emptyRetrieval;
  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    return {
      embeddingProfileId: typeof parsed.embedding_profile_id === "string" ? parsed.embedding_profile_id : null,
      mineruProfileId: typeof parsed.mineru_profile_id === "string" ? parsed.mineru_profile_id : null,
    };
  } catch {
    return emptyRetrieval;
  }
}

export function createIndexRequest(profile: { id: string; model_id: string | null }): IndexRebuildRequest {
  return {
    provider_profile_id: profile.id,
    model_id: profile.model_id ?? defaultEmbeddingModel,
    dimension: defaultEmbeddingDimension,
  };
}
