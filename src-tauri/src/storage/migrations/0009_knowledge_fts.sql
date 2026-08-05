DROP TABLE knowledge_chunks_fts;

CREATE VIRTUAL TABLE knowledge_chunks_fts USING fts5(
  workspace_id UNINDEXED,
  knowledge_base_id UNINDEXED,
  document_id UNINDEXED,
  version_id UNINDEXED,
  chunk_id UNINDEXED,
  title_path,
  source_name,
  grade_aliases,
  text,
  tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO knowledge_chunks_fts (
  workspace_id, knowledge_base_id, document_id, version_id, chunk_id,
  title_path, source_name, grade_aliases, text
)
SELECT
  chunks.workspace_id,
  documents.knowledge_base_id,
  documents.id,
  chunks.version_id,
  chunks.id,
  '',
  documents.display_name,
  chunks.text,
  chunks.text
FROM knowledge_chunks AS chunks
JOIN knowledge_document_versions AS versions
  ON versions.workspace_id = chunks.workspace_id AND versions.id = chunks.version_id
JOIN knowledge_source_documents AS documents
  ON documents.workspace_id = versions.workspace_id AND documents.id = versions.document_id;
