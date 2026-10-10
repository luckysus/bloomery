-- 第 80 章数据模型：补齐 Agent / Tool / Skill / PredictionTask / OptimizationTask / Experiment。
-- 本迁移只新增表与索引，不修改、不清空任何既有数据。

CREATE TABLE agents (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  preset INTEGER NOT NULL CHECK (preset IN (0, 1)),
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  status TEXT NOT NULL CHECK (status IN ('ready', 'disabled', 'error')),
  system_prompt TEXT NOT NULL DEFAULT '',
  provider_id TEXT,
  tool_ids_json TEXT NOT NULL DEFAULT '[]',
  permission_restrictions_json TEXT NOT NULL DEFAULT '{}',
  limits_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);

CREATE INDEX idx_agents_workspace_enabled
  ON agents (workspace_id, enabled, name);

CREATE TABLE tools (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL CHECK (source IN ('builtin', 'mcp', 'domain')),
  risk TEXT NOT NULL CHECK (risk IN ('automatic', 'confirmation_required', 'dangerous')),
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);

CREATE INDEX idx_tools_workspace_source
  ON tools (workspace_id, source, name);

CREATE TABLE skills (
  workspace_id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  version TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  source_scope TEXT NOT NULL,
  source_path TEXT NOT NULL,
  tags_json TEXT NOT NULL DEFAULT '[]',
  compatibility_json TEXT NOT NULL DEFAULT '[]',
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, name)
);

CREATE INDEX idx_skills_workspace_enabled
  ON skills (workspace_id, enabled, name);

CREATE TABLE prediction_tasks (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  background_task_id TEXT,
  dataset_id TEXT,
  model_id TEXT,
  state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
  request_json TEXT NOT NULL DEFAULT '{}',
  result_json TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);

CREATE INDEX idx_prediction_tasks_workspace_created
  ON prediction_tasks (workspace_id, created_at DESC);

CREATE TABLE optimization_tasks (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  background_task_id TEXT,
  dataset_id TEXT,
  model_id TEXT,
  direction TEXT NOT NULL CHECK (direction IN ('minimize', 'maximize')),
  method TEXT,
  trials_requested INTEGER NOT NULL CHECK (trials_requested >= 0),
  state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
  request_json TEXT NOT NULL DEFAULT '{}',
  result_json TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);

CREATE INDEX idx_optimization_tasks_workspace_created
  ON optimization_tasks (workspace_id, created_at DESC);

CREATE TABLE experiments (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  conversation_id TEXT,
  run_id TEXT,
  title TEXT NOT NULL,
  objective TEXT NOT NULL DEFAULT '',
  variables_json TEXT NOT NULL DEFAULT '[]',
  recommendation_json TEXT,
  state TEXT NOT NULL CHECK (state IN ('draft', 'proposed', 'accepted', 'rejected')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);

CREATE INDEX idx_experiments_workspace_created
  ON experiments (workspace_id, created_at DESC);
