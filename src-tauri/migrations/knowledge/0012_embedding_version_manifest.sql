ALTER TABLE document_versions
    ADD COLUMN IF NOT EXISTS embedding_profile_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS embedding_model_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS embedding_dimension INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS expected_asset_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS expected_chunk_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS manifest_sealed BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE document_versions DROP CONSTRAINT IF EXISTS document_versions_embedding_dimension_check;
ALTER TABLE document_versions ADD CONSTRAINT document_versions_embedding_dimension_check
    CHECK (embedding_dimension >= 0);
