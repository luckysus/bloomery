CREATE TABLE steel_datasets (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  source_name TEXT NOT NULL,
  source_path TEXT NOT NULL,
  source_sha256 TEXT NOT NULL,
  format TEXT NOT NULL,
  selected_sheet TEXT NOT NULL,
  row_count INTEGER NOT NULL CHECK (row_count >= 0),
  column_count INTEGER NOT NULL CHECK (column_count >= 0),
  truncated INTEGER NOT NULL CHECK (truncated IN (0, 1)),
  mapping_state TEXT NOT NULL CHECK (mapping_state IN ('draft', 'ready')),
  preview_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, id),
  UNIQUE (workspace_id, source_sha256, selected_sheet)
);

CREATE TABLE steel_dataset_columns (
  workspace_id TEXT NOT NULL,
  dataset_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
  original_name TEXT NOT NULL,
  duplicate INTEGER NOT NULL CHECK (duplicate IN (0, 1)),
  inferred_type TEXT NOT NULL,
  canonical_field TEXT,
  unit TEXT,
  non_empty_count INTEGER NOT NULL CHECK (non_empty_count >= 0),
  missing_count INTEGER NOT NULL CHECK (missing_count >= 0),
  invalid_count INTEGER NOT NULL CHECK (invalid_count >= 0),
  min_value REAL,
  max_value REAL,
  PRIMARY KEY (workspace_id, dataset_id, ordinal),
  FOREIGN KEY (workspace_id, dataset_id)
    REFERENCES steel_datasets (workspace_id, id)
    ON DELETE CASCADE
);

CREATE INDEX idx_steel_datasets_workspace_updated
  ON steel_datasets (workspace_id, updated_at DESC);

CREATE INDEX idx_steel_dataset_columns_workspace_dataset
  ON steel_dataset_columns (workspace_id, dataset_id, ordinal);
