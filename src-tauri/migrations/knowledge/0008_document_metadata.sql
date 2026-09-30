ALTER TABLE source_documents
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS source_documents_metadata_gin_idx
    ON source_documents USING GIN (metadata);
