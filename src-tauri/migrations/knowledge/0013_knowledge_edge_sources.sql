ALTER TABLE knowledge_edges
    ADD COLUMN IF NOT EXISTS source_document_id UUID REFERENCES source_documents(id);

ALTER TABLE knowledge_edges DROP CONSTRAINT IF EXISTS knowledge_edges_endpoint_check;
ALTER TABLE knowledge_edges DROP CONSTRAINT IF EXISTS knowledge_edges_source_or_target_check;
ALTER TABLE knowledge_edges DROP CONSTRAINT IF EXISTS knowledge_edges_check;
ALTER TABLE knowledge_edges ADD CONSTRAINT knowledge_edges_endpoint_check
    CHECK (source_page_id IS NOT NULL OR target_page_id IS NOT NULL OR source_document_id IS NOT NULL);

CREATE INDEX IF NOT EXISTS knowledge_edges_source_document_idx
    ON knowledge_edges (source_document_id)
    WHERE source_document_id IS NOT NULL AND deleted_at IS NULL;
