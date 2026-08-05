CREATE TABLE knowledge_vectors (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  provider_profile_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  dimension INTEGER NOT NULL CHECK (dimension > 0),
  normalized_text_sha256 TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  vector_blob BLOB NOT NULL,
  vector_sha256 TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE (
    workspace_id, provider_profile_id, model_id, dimension,
    normalized_text_sha256, policy_version
  ),
  CHECK (length(vector_blob) = dimension * 4)
);

CREATE INDEX idx_knowledge_vectors_identity
  ON knowledge_vectors(
    workspace_id, provider_profile_id, model_id, dimension,
    normalized_text_sha256, policy_version
  );
