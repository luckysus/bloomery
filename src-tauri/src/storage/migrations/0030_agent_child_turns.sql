CREATE TABLE IF NOT EXISTS agent_child_turns (
  child_turn_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  parent_turn_id TEXT NOT NULL,
  parent_conversation_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN (
    'created', 'preparing', 'generating', 'awaiting_permission',
    'executing_tools', 'verifying', 'completing',
    'completed', 'cancelled', 'failed', 'interrupted'
  )),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE (workspace_id, child_turn_id),
  FOREIGN KEY (workspace_id, parent_turn_id, parent_conversation_id)
    REFERENCES agent_runs(workspace_id, id, conversation_id) ON DELETE CASCADE,
  CHECK (
    (state IN ('completed', 'cancelled', 'failed', 'interrupted')) =
    (completed_at IS NOT NULL)
  )
);

CREATE INDEX IF NOT EXISTS idx_agent_child_turns_parent
  ON agent_child_turns(workspace_id, parent_turn_id, created_at, child_turn_id);

CREATE TABLE IF NOT EXISTS agent_child_turn_events (
  event_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  child_turn_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK (sequence > 0),
  protocol_version INTEGER NOT NULL CHECK (protocol_version > 0),
  timestamp TEXT NOT NULL,
  event_json TEXT NOT NULL CHECK (length(event_json) > 0),
  UNIQUE (child_turn_id, sequence),
  FOREIGN KEY (workspace_id, child_turn_id)
    REFERENCES agent_child_turns(workspace_id, child_turn_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_child_turn_events_replay
  ON agent_child_turn_events(workspace_id, child_turn_id, sequence);
