ALTER TABLE database_connections ADD COLUMN last_checked_at TEXT;
ALTER TABLE database_connections ADD COLUMN last_latency_ms INTEGER;
ALTER TABLE database_connections ADD COLUMN last_version TEXT;
ALTER TABLE database_connections ADD COLUMN last_error TEXT;

CREATE TABLE database_query_results (
  task_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  database_name TEXT NOT NULL DEFAULT '',
  query_text TEXT NOT NULL,
  row_count INTEGER NOT NULL,
  truncated INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  csv_path TEXT NOT NULL,
  columns_json TEXT NOT NULL,
  rows_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_database_query_results_workspace_created
  ON database_query_results(workspace_id, created_at DESC, task_id);
