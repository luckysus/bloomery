ALTER TABLE chunk_embeddings
    ADD COLUMN IF NOT EXISTS knowledge_base_id UUID,
    ADD COLUMN IF NOT EXISTS document_id UUID,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE chunk_embeddings e
SET knowledge_base_id = d.knowledge_base_id,
    document_id = d.id,
    metadata = COALESCE(e.metadata, '{}'::jsonb) || jsonb_build_object('source_location', c.source_location)
FROM document_chunks c
JOIN document_versions v ON v.id = c.version_id
JOIN source_documents d ON d.id = v.document_id
WHERE e.chunk_id = c.id
  AND (e.knowledge_base_id IS NULL OR e.document_id IS NULL);

ALTER TABLE chunk_embeddings
    ALTER COLUMN knowledge_base_id SET NOT NULL,
    ALTER COLUMN document_id SET NOT NULL;

ALTER TABLE chunk_embeddings
    ADD CONSTRAINT chunk_embeddings_knowledge_base_fk
      FOREIGN KEY (knowledge_base_id) REFERENCES knowledge_bases(id),
    ADD CONSTRAINT chunk_embeddings_document_fk
      FOREIGN KEY (document_id) REFERENCES source_documents(id);

CREATE INDEX IF NOT EXISTS chunk_embeddings_scope_idx
    ON chunk_embeddings (knowledge_base_id, document_id, model_id, dimension);
