ALTER TABLE ingestion_jobs DROP CONSTRAINT IF EXISTS ingestion_jobs_state_check;
ALTER TABLE ingestion_jobs ADD CONSTRAINT ingestion_jobs_state_check
  CHECK (state IN ('queued', 'uploading', 'parsing', 'chunking', 'embedding', 'indexing', 'pending', 'running', 'completed', 'failed', 'retrying', 'quarantined', 'cancelled'));
