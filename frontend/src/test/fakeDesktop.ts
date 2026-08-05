import type { EvidencePack, KnowledgeBaseRecord } from "../bridge/desktop";

export function createFakeDesktop() {
  const knowledgeBases: KnowledgeBaseRecord[] = [];
  return {
    initialize: async () => undefined,
    listKnowledgeBases: async () => knowledgeBases,
    createKnowledgeBase: async (name: string) => {
      const now = new Date().toISOString();
      const record = { id: `kb-${knowledgeBases.length + 1}`, name, created_at: now, updated_at: now };
      knowledgeBases.push(record);
      return record;
    },
    queryLocalKnowledge: async (): Promise<EvidencePack> => ({
      id: "evidence-1",
      workspace_id: "local",
      query: "",
      configuration: {
        knowledge_base_ids: [],
        lexical_limit: 40,
        dense_limit: 40,
        candidate_limit: 20,
        rrf_k: 60,
        embedding_provider_profile_id: "",
        embedding_model_id: "",
        rerank_provider_profile_id: null,
        rerank_model_id: null,
        rerank_degradation: null,
      },
      evidence: [],
      created_at: new Date().toISOString(),
    }),
  };
}
