ALTER TABLE knowledge_bases
    ADD COLUMN IF NOT EXISTS owner_id TEXT NOT NULL DEFAULT 'local-owner';

CREATE INDEX IF NOT EXISTS idx_knowledge_bases_owner_id
    ON knowledge_bases (owner_id);
