ALTER TABLE background_tasks ADD COLUMN owner TEXT;
ALTER TABLE background_tasks ADD COLUMN claim_token TEXT;
ALTER TABLE background_tasks ADD COLUMN lease_expires_at TEXT;

CREATE TABLE task_claim_tokens (
  token TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_background_tasks_claim_token
  ON background_tasks(claim_token)
  WHERE claim_token IS NOT NULL;
