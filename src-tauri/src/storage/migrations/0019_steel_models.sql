CREATE TABLE steel_models (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  lineage_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('linear_artifact', 'onnx')),
  version INTEGER NOT NULL CHECK (version >= 1),
  source_task_id TEXT,
  model_sha256 TEXT NOT NULL,
  manifest_json TEXT NOT NULL,
  artifact_json TEXT,
  model_base64 TEXT,
  is_active INTEGER NOT NULL CHECK (is_active IN (0, 1)),
  created_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id),
  UNIQUE (workspace_id, lineage_id, version),
  CHECK (
    (artifact_json IS NOT NULL AND model_base64 IS NULL AND kind = 'linear_artifact')
    OR (artifact_json IS NULL AND model_base64 IS NOT NULL AND kind = 'onnx')
  )
);

CREATE INDEX idx_steel_models_workspace_lineage
  ON steel_models (workspace_id, lineage_id, version DESC);
