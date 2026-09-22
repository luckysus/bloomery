ALTER TABLE source_documents
    ADD COLUMN IF NOT EXISTS storage_path TEXT NOT NULL DEFAULT '';
