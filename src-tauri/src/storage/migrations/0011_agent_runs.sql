CREATE UNIQUE INDEX idx_conversations_workspace_identity
  ON conversations(workspace_id, id);

CREATE UNIQUE INDEX idx_messages_workspace_conversation_identity
  ON messages(workspace_id, conversation_id, id);

CREATE TABLE agent_runs (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  user_message_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN (
    'created', 'preparing', 'generating', 'awaiting_permission',
    'executing_tools', 'verifying', 'completing',
    'completed', 'cancelled', 'failed', 'interrupted'
  )),
  next_sequence INTEGER NOT NULL DEFAULT 1 CHECK (next_sequence >= 1),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE (workspace_id, id, conversation_id),
  FOREIGN KEY (workspace_id, conversation_id)
    REFERENCES conversations(workspace_id, id) ON DELETE CASCADE,
  FOREIGN KEY (workspace_id, conversation_id, user_message_id)
    REFERENCES messages(workspace_id, conversation_id, id) ON DELETE CASCADE,
  CHECK (
    (state IN ('completed', 'cancelled', 'failed', 'interrupted')) =
    (completed_at IS NOT NULL)
  )
);

CREATE INDEX idx_agent_runs_workspace_conversation
  ON agent_runs(workspace_id, conversation_id, created_at, id);

CREATE TABLE agent_run_events (
  event_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK (sequence > 0),
  protocol_version INTEGER NOT NULL CHECK (protocol_version > 0),
  timestamp TEXT NOT NULL,
  event_json TEXT NOT NULL CHECK (length(event_json) > 0),
  UNIQUE (run_id, sequence),
  FOREIGN KEY (workspace_id, run_id, conversation_id)
    REFERENCES agent_runs(workspace_id, id, conversation_id) ON DELETE CASCADE
);

CREATE INDEX idx_agent_run_events_replay
  ON agent_run_events(workspace_id, run_id, sequence);
