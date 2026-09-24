CREATE TABLE IF NOT EXISTS agent_run_checkpoints (
  run_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  reason TEXT NOT NULL CHECK (reason IN ('model_call', 'assistant_result', 'assistant_error')),
  checkpoint_json TEXT NOT NULL CHECK (length(checkpoint_json) > 0),
  updated_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_run_checkpoints_workspace
  ON agent_run_checkpoints(workspace_id, updated_at);
