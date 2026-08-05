ALTER TABLE knowledge_document_versions
  ADD COLUMN manifest_sealed INTEGER NOT NULL DEFAULT 1 CHECK (manifest_sealed IN (0, 1));

DROP TRIGGER knowledge_document_versions_immutable;

CREATE TRIGGER knowledge_document_versions_immutable
BEFORE UPDATE OF workspace_id, document_id, content_sha256, mime_type, parser, parser_version,
  chunk_policy_version, embedding_profile_id, embedding_model_id, embedding_dimension, created_at
ON knowledge_document_versions
BEGIN
  SELECT RAISE(ABORT, 'immutable_document_version');
END;

CREATE TRIGGER knowledge_document_manifest_pending_insert
BEFORE INSERT ON knowledge_document_versions
WHEN NEW.manifest_sealed = 0
  AND (NEW.expected_asset_count != 0 OR NEW.expected_chunk_count != 0)
BEGIN
  SELECT RAISE(ABORT, 'invalid_pending_document_manifest');
END;

CREATE TRIGGER knowledge_document_manifest_seal_once
BEFORE UPDATE OF expected_asset_count, expected_chunk_count, manifest_sealed
ON knowledge_document_versions
WHEN NOT (
  OLD.manifest_sealed = 0
  AND NEW.manifest_sealed = 1
  AND OLD.expected_asset_count = 0
  AND OLD.expected_chunk_count = 0
  AND NEW.expected_asset_count >= 0
  AND NEW.expected_chunk_count > 0
  AND NOT EXISTS (
    SELECT 1 FROM knowledge_assets WHERE version_id = OLD.id
  )
  AND NOT EXISTS (
    SELECT 1 FROM knowledge_chunks WHERE version_id = OLD.id
  )
)
BEGIN
  SELECT RAISE(ABORT, 'immutable_document_version');
END;
