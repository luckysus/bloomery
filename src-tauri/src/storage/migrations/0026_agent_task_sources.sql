CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_runs_workspace_id
  ON agent_runs(workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_background_tasks_workspace_id
  ON background_tasks(workspace_id, id);

CREATE TABLE IF NOT EXISTS agent_task_sources (
  workspace_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  tool_call_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  PRIMARY KEY (workspace_id, run_id, tool_call_id),
  UNIQUE (workspace_id, task_id),
  FOREIGN KEY (workspace_id, run_id)
    REFERENCES agent_runs(workspace_id, id) ON DELETE CASCADE,
  FOREIGN KEY (workspace_id, task_id)
    REFERENCES background_tasks(workspace_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_task_sources_task
  ON agent_task_sources(workspace_id, task_id);
