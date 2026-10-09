ALTER TABLE agent_task_sources ADD COLUMN delivered_at TEXT;
CREATE INDEX idx_agent_task_sources_delivery
  ON agent_task_sources(workspace_id, run_id, delivered_at);

ALTER TABLE cron_jobs ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1));
ALTER TABLE cron_jobs ADD COLUMN conversation_id TEXT;
ALTER TABLE cron_jobs ADD COLUMN agent_id TEXT NOT NULL DEFAULT 'master';
ALTER TABLE cron_outbox ADD COLUMN conversation_id TEXT;
ALTER TABLE cron_outbox ADD COLUMN agent_id TEXT NOT NULL DEFAULT 'master';
ALTER TABLE cron_outbox ADD COLUMN run_id TEXT;
ALTER TABLE cron_outbox ADD COLUMN error_message TEXT;
CREATE UNIQUE INDEX idx_cron_outbox_run
  ON cron_outbox(workspace_id, run_id) WHERE run_id IS NOT NULL;
