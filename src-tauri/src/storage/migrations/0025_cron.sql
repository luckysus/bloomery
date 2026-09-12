CREATE TABLE cron_jobs (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  expression TEXT NOT NULL,
  timezone TEXT NOT NULL,
  prompt TEXT NOT NULL,
  identity TEXT NOT NULL,
  recurring INTEGER NOT NULL CHECK (recurring IN (0, 1)),
  durable INTEGER NOT NULL CHECK (durable IN (0, 1)),
  next_run_at_utc TEXT NOT NULL,
  last_slot_at_utc TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_cron_jobs_due ON cron_jobs(workspace_id, durable, next_run_at_utc);
CREATE TABLE cron_outbox (
  event_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  slot_at_utc TEXT NOT NULL,
  identity TEXT NOT NULL,
  prompt TEXT NOT NULL,
  acknowledged_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(workspace_id, job_id, slot_at_utc)
);
CREATE INDEX idx_cron_outbox_pending ON cron_outbox(workspace_id, acknowledged_at, slot_at_utc);
