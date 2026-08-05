CREATE TABLE provider_profiles (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  model_id TEXT,
  secret_ref TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_provider_profiles_workspace
  ON provider_profiles(workspace_id, enabled, kind, display_name);

CREATE TABLE provider_defaults (
  workspace_id TEXT NOT NULL,
  capability TEXT NOT NULL,
  profile_id TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, capability),
  FOREIGN KEY (profile_id) REFERENCES provider_profiles(id) ON DELETE CASCADE
);
