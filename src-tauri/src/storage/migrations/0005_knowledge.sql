CREATE TABLE knowledge_bases (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (workspace_id, name)
);

CREATE INDEX idx_knowledge_bases_workspace
  ON knowledge_bases(workspace_id, updated_at DESC, id);

CREATE TABLE knowledge_source_documents (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  knowledge_base_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  active_version_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (knowledge_base_id) REFERENCES knowledge_bases(id) ON DELETE CASCADE
);

CREATE INDEX idx_knowledge_sources_workspace
  ON knowledge_source_documents(workspace_id, knowledge_base_id, updated_at DESC, id);

CREATE TABLE knowledge_document_versions (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  document_id TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  parser TEXT NOT NULL,
  parser_version TEXT NOT NULL,
  chunk_policy_version TEXT NOT NULL,
  embedding_profile_id TEXT NOT NULL,
  embedding_model_id TEXT NOT NULL,
  embedding_dimension INTEGER NOT NULL CHECK (embedding_dimension > 0),
  expected_asset_count INTEGER NOT NULL CHECK (expected_asset_count >= 0),
  expected_chunk_count INTEGER NOT NULL CHECK (expected_chunk_count >= 0),
  created_at TEXT NOT NULL,
  activated_at TEXT,
  UNIQUE (document_id, content_sha256),
  FOREIGN KEY (document_id) REFERENCES knowledge_source_documents(id) ON DELETE CASCADE
);

CREATE INDEX idx_knowledge_versions_document
  ON knowledge_document_versions(workspace_id, document_id, created_at DESC, id);

CREATE TRIGGER knowledge_document_versions_immutable
BEFORE UPDATE OF workspace_id, document_id, content_sha256, mime_type, parser, parser_version,
  chunk_policy_version, embedding_profile_id, embedding_model_id,
  embedding_dimension, expected_asset_count, expected_chunk_count, created_at
ON knowledge_document_versions
BEGIN
  SELECT RAISE(ABORT, 'immutable_document_version');
END;

CREATE TABLE knowledge_assets (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  version_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  media_type TEXT NOT NULL,
  source_location_json TEXT,
  created_at TEXT NOT NULL,
  UNIQUE (version_id, storage_key),
  FOREIGN KEY (version_id) REFERENCES knowledge_document_versions(id) ON DELETE CASCADE
);

CREATE INDEX idx_knowledge_assets_version
  ON knowledge_assets(workspace_id, version_id, id);

CREATE TABLE knowledge_chunks (
  id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  version_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
  text TEXT NOT NULL,
  source_location_json TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (version_id, id),
  UNIQUE (version_id, ordinal),
  FOREIGN KEY (version_id) REFERENCES knowledge_document_versions(id) ON DELETE CASCADE
);

CREATE INDEX idx_knowledge_chunks_version
  ON knowledge_chunks(workspace_id, version_id, ordinal, id);

CREATE TABLE knowledge_chunk_embeddings (
  workspace_id TEXT NOT NULL,
  version_id TEXT NOT NULL,
  chunk_id TEXT NOT NULL,
  provider_profile_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  dimension INTEGER NOT NULL CHECK (dimension > 0),
  normalized_text_sha256 TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  vector_key TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (version_id, chunk_id),
  FOREIGN KEY (version_id, chunk_id) REFERENCES knowledge_chunks(version_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_knowledge_embeddings_version
  ON knowledge_chunk_embeddings(workspace_id, version_id, chunk_id);

CREATE VIRTUAL TABLE knowledge_chunks_fts USING fts5(
  workspace_id UNINDEXED,
  version_id UNINDEXED,
  chunk_id UNINDEXED,
  text,
  tokenize = 'unicode61'
);

CREATE TABLE knowledge_vector_watermarks (
  workspace_id TEXT NOT NULL,
  version_id TEXT PRIMARY KEY,
  provider_profile_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  dimension INTEGER NOT NULL CHECK (dimension > 0),
  expected_count INTEGER NOT NULL CHECK (expected_count >= 0),
  indexed_count INTEGER NOT NULL CHECK (indexed_count >= 0 AND indexed_count <= expected_count),
  index_version TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (version_id) REFERENCES knowledge_document_versions(id) ON DELETE CASCADE
);

CREATE TABLE knowledge_ingest_attempts (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  document_id TEXT NOT NULL,
  version_id TEXT,
  task_id TEXT,
  state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'cancelled', 'interrupted')),
  error_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  finished_at TEXT,
  CHECK ((state = 'failed') = (error_code IS NOT NULL)),
  FOREIGN KEY (document_id) REFERENCES knowledge_source_documents(id) ON DELETE CASCADE,
  FOREIGN KEY (version_id) REFERENCES knowledge_document_versions(id) ON DELETE SET NULL
);

CREATE INDEX idx_knowledge_attempts_document
  ON knowledge_ingest_attempts(workspace_id, document_id, created_at DESC, id);
