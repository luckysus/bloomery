ALTER TABLE knowledge_edges
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS knowledge_edges_active_idx
    ON knowledge_edges (knowledge_base_id, created_at)
    WHERE deleted_at IS NULL;
