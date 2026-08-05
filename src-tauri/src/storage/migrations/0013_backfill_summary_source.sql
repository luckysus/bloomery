-- Backfill source_message_ids for conversation_summaries that don't have it yet
-- This migration runs after migration 12 which added the column but may not have filled all existing summaries

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
WHERE covered_message_id IS NOT NULL
  AND source_message_ids_json = '[]'
  AND source_message_ids_json IS NOT NULL;
