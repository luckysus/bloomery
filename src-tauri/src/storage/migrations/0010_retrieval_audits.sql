CREATE TABLE retrieval_audits (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL CHECK (length(trim(workspace_id)) > 0),
  query TEXT NOT NULL CHECK (length(trim(query)) > 0),
  configuration_json TEXT NOT NULL CHECK (json_valid(configuration_json)),
  evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
  created_at TEXT NOT NULL
);

CREATE INDEX idx_retrieval_audits_workspace
  ON retrieval_audits(workspace_id, created_at DESC, id);

CREATE TRIGGER retrieval_audits_immutable
BEFORE UPDATE ON retrieval_audits
BEGIN
  SELECT RAISE(ABORT, 'immutable_retrieval_audit');
END;
