INSERT INTO migration_schema_snapshots
  (migration_version, object_type, name, table_name, sql, captured_at)
SELECT 2, type, name, tbl_name, sql, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM sqlite_schema
WHERE sql IS NOT NULL;

DROP INDEX IF EXISTS idx_conversations_user_updated;
DROP INDEX IF EXISTS idx_messages_conversation_created;
DROP INDEX IF EXISTS idx_memories_user_enabled;

ALTER TABLE conversations RENAME TO legacy_conversations_0001;
ALTER TABLE messages RENAME TO legacy_messages_0001;
ALTER TABLE conversation_drafts RENAME TO legacy_conversation_drafts_0001;
ALTER TABLE memories RENAME TO legacy_memories_0001;
ALTER TABLE conversation_summaries RENAME TO legacy_conversation_summaries_0001;
ALTER TABLE settings RENAME TO legacy_settings_0001;

CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_conversations_workspace_updated
  ON conversations(workspace_id, archived, updated_at);

CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  response_json TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_messages_conversation_created
  ON messages(workspace_id, conversation_id, created_at);

CREATE TABLE conversation_drafts (
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  content TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, conversation_id)
);

CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  type TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL,
  tags_json TEXT NOT NULL DEFAULT '[]',
  enabled INTEGER NOT NULL DEFAULT 1,
  archived_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_memories_workspace_enabled
  ON memories(workspace_id, enabled, archived_at, updated_at);

CREATE TABLE conversation_summaries (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  summary TEXT NOT NULL,
  covered_message_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE settings (
  workspace_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, key)
);

CREATE TABLE legacy_settings_archive (
  source_user_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  migrated_at TEXT NOT NULL,
  PRIMARY KEY (source_user_id, key)
);

INSERT INTO conversations
  (id, workspace_id, title, created_at, updated_at, pinned, archived)
SELECT id, 'local', title, created_at, updated_at, pinned, archived
FROM legacy_conversations_0001;

INSERT INTO messages
  (id, workspace_id, conversation_id, role, content, response_json, created_at)
SELECT id, 'local', conversation_id, role, content, response_json, created_at
FROM legacy_messages_0001;

INSERT INTO conversation_drafts
  (workspace_id, conversation_id, content, updated_at)
SELECT 'local', conversation_id, content, updated_at
FROM legacy_conversation_drafts_0001;

INSERT INTO memories
  (id, workspace_id, scope, type, title, description, body, tags_json, enabled,
   archived_at, created_at, updated_at)
SELECT id, 'local', scope, type, title, description, body, tags_json, enabled,
       archived_at, created_at, updated_at
FROM legacy_memories_0001;

INSERT INTO conversation_summaries
  (id, workspace_id, conversation_id, summary, covered_message_id, created_at, updated_at)
SELECT id, 'local', conversation_id, summary, covered_message_id, created_at, updated_at
FROM legacy_conversation_summaries_0001;

INSERT INTO legacy_settings_archive
  (source_user_id, key, value_json, updated_at, migrated_at)
SELECT user_id, key, value_json, updated_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM legacy_settings_0001;

INSERT INTO settings (workspace_id, key, value_json, updated_at)
SELECT 'local', setting.key, setting.value_json, setting.updated_at
FROM legacy_settings_0001 AS setting
WHERE setting.key <> 'cloud_api_base'
  AND NOT EXISTS (
    SELECT 1
    FROM legacy_settings_0001 AS newer
    WHERE newer.key = setting.key
      AND (
        newer.updated_at > setting.updated_at
        OR (newer.updated_at = setting.updated_at AND newer.user_id > setting.user_id)
      )
  );

DROP TABLE legacy_conversations_0001;
DROP TABLE legacy_messages_0001;
DROP TABLE legacy_conversation_drafts_0001;
DROP TABLE legacy_memories_0001;
DROP TABLE legacy_conversation_summaries_0001;
DROP TABLE legacy_settings_0001;
