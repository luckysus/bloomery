CREATE TABLE IF NOT EXISTS permission_rules (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  tool_id TEXT NOT NULL,
  tool_version TEXT NOT NULL,
  source_json TEXT NOT NULL,
  action TEXT NOT NULL CHECK (action = 'execute'),
  scope_json TEXT NOT NULL,
  effect TEXT NOT NULL CHECK (effect IN ('allow', 'deny')),
  created_at TEXT NOT NULL,
  revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_permission_rules_workspace_active
  ON permission_rules(workspace_id, revoked_at, created_at, id);
