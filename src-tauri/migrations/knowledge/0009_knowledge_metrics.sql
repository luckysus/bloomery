ALTER TABLE retrieval_audits
  ADD COLUMN IF NOT EXISTS duration_ms BIGINT NOT NULL DEFAULT 0;

ALTER TABLE retrieval_audits
  DROP CONSTRAINT IF EXISTS retrieval_audits_duration_ms_check;

ALTER TABLE retrieval_audits
  ADD CONSTRAINT retrieval_audits_duration_ms_check CHECK (duration_ms >= 0);
