CREATE TABLE mcp_servers (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  server_id TEXT NOT NULL,
  transport TEXT NOT NULL CHECK (transport IN ('stdio', 'streamable_http')),
  url TEXT,
  executable TEXT,
  args_json TEXT NOT NULL,
  working_directory TEXT,
  inherited_env_json TEXT NOT NULL,
  env_names_json TEXT NOT NULL,
  timeout_ms INTEGER NOT NULL CHECK (timeout_ms BETWEEN 100 AND 600000),
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_mcp_servers_workspace_updated
  ON mcp_servers(workspace_id, updated_at DESC, id);
