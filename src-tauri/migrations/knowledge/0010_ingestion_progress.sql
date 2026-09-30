ALTER TABLE ingestion_jobs
    ADD COLUMN IF NOT EXISTS progress INTEGER NOT NULL DEFAULT 0;

ALTER TABLE ingestion_jobs DROP CONSTRAINT IF EXISTS ingestion_jobs_progress_check;
ALTER TABLE ingestion_jobs ADD CONSTRAINT ingestion_jobs_progress_check
    CHECK (progress >= 0 AND progress <= 100);

UPDATE ingestion_jobs
SET progress = CASE state
    WHEN 'completed' THEN 100
    WHEN 'uploading' THEN 20
    WHEN 'parsing' THEN 42
    WHEN 'chunking' THEN 62
    WHEN 'embedding' THEN 80
    WHEN 'indexing' THEN 92
    WHEN 'running' THEN 55
    WHEN 'retrying' THEN 30
    WHEN 'failed' THEN 100
    WHEN 'quarantined' THEN 100
    WHEN 'cancelled' THEN 100
    ELSE progress
END;
