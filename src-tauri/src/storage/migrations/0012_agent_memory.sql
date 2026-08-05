ALTER TABLE memories ADD COLUMN source_message_id TEXT;
ALTER TABLE memories ADD COLUMN source_run_id TEXT;
ALTER TABLE memories ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0
  CHECK (confidence >= 0.0 AND confidence <= 1.0);
ALTER TABLE memories ADD COLUMN status TEXT NOT NULL DEFAULT 'confirmed'
  CHECK (status IN ('pending', 'confirmed', 'rejected'));
ALTER TABLE memories ADD COLUMN dedup_key TEXT NOT NULL DEFAULT '';

UPDATE memories SET dedup_key = 'legacy:' || id WHERE dedup_key = '';

CREATE INDEX idx_memories_workspace_dedup
  ON memories(workspace_id, dedup_key);

ALTER TABLE conversation_summaries
  ADD COLUMN source_message_ids_json TEXT NOT NULL DEFAULT '[]';

UPDATE conversation_summaries AS summary
SET source_message_ids_json = COALESCE((
  SELECT json_group_array(id)
  FROM (
    SELECT message.id
    FROM messages AS message
    JOIN messages AS anchor
      ON anchor.workspace_id = summary.workspace_id
     AND anchor.conversation_id = summary.conversation_id
     AND anchor.id = summary.covered_message_id
    WHERE message.workspace_id = summary.workspace_id
      AND message.conversation_id = summary.conversation_id
      AND (
        message.created_at < anchor.created_at
        OR (message.created_at = anchor.created_at AND message.rowid <= anchor.rowid)
      )
    ORDER BY message.created_at ASC, message.rowid ASC
  )
), '[]')
WHERE covered_message_id IS NOT NULL;
