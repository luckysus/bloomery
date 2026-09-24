CREATE TABLE IF NOT EXISTS agent_run_turn_snapshots (
  run_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  snapshot_json TEXT NOT NULL CHECK (length(snapshot_json) > 0),
  updated_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_run_turn_snapshots_workspace
  ON agent_run_turn_snapshots(workspace_id, updated_at);
