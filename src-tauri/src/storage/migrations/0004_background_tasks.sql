CREATE TABLE background_tasks (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN (
    'queued', 'running', 'waiting_external', 'paused',
    'completed', 'failed', 'cancelled', 'interrupted'
  )),
  payload_json TEXT NOT NULL,
  checkpoint_json TEXT,
  attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
  next_run_at TEXT,
  progress INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
  error_code TEXT,
  cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CHECK ((state = 'failed') = (error_code IS NOT NULL)),
  CHECK (state <> 'completed' OR (progress = 100 AND next_run_at IS NULL))
);

CREATE INDEX idx_background_tasks_workspace
  ON background_tasks(workspace_id, created_at ASC, id ASC);

CREATE INDEX idx_background_tasks_due_queued
  ON background_tasks(workspace_id, state, cancel_requested, next_run_at, created_at, id);
