CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS knowledge_bases (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS source_documents (
    id UUID PRIMARY KEY,
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases(id),
    display_name TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_path TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    UNIQUE (knowledge_base_id, source_path, content_sha256)
);

CREATE TABLE IF NOT EXISTS document_versions (
    id UUID PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES source_documents(id),
    content_sha256 TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    parser TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    chunk_policy_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at TIMESTAMPTZ,
    UNIQUE (document_id, content_sha256)
);

CREATE TABLE IF NOT EXISTS document_assets (
    id UUID PRIMARY KEY,
    version_id UUID NOT NULL REFERENCES document_versions(id) ON DELETE CASCADE,
    asset_kind TEXT NOT NULL,
    local_path TEXT NOT NULL,
    content_sha256 TEXT,
    mime_type TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (version_id, asset_kind, local_path)
);

CREATE INDEX IF NOT EXISTS document_assets_version_idx
    ON document_assets (version_id);

CREATE TABLE IF NOT EXISTS document_chunks (
    id UUID PRIMARY KEY,
    version_id UUID NOT NULL REFERENCES document_versions(id),
    parent_id UUID REFERENCES document_chunks(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    title_path TEXT NOT NULL DEFAULT '',
    text TEXT NOT NULL,
    source_location JSONB NOT NULL DEFAULT '{}'::jsonb,
    search_vector TSVECTOR GENERATED ALWAYS AS (to_tsvector('simple', coalesce(title_path, '') || ' ' || text)) STORED,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (version_id, ordinal)
);

CREATE INDEX IF NOT EXISTS document_chunks_search_idx ON document_chunks USING GIN (search_vector);

CREATE TABLE IF NOT EXISTS chunk_embeddings (
    chunk_id UUID PRIMARY KEY REFERENCES document_chunks(id),
    model_id TEXT NOT NULL,
    dimension INTEGER NOT NULL CHECK (dimension > 0),
    embedding vector NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS wiki_pages (
    id UUID PRIMARY KEY,
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    body_markdown TEXT NOT NULL,
    source_document_id UUID REFERENCES source_documents(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    UNIQUE (knowledge_base_id, slug)
);

CREATE TABLE IF NOT EXISTS wiki_page_revisions (
    id UUID PRIMARY KEY,
    page_id UUID NOT NULL REFERENCES wiki_pages(id),
    revision INTEGER NOT NULL CHECK (revision > 0),
    title TEXT NOT NULL,
    body_markdown TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (page_id, revision)
);

CREATE TABLE IF NOT EXISTS tags (
    id UUID PRIMARY KEY,
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases(id),
    name TEXT NOT NULL,
    UNIQUE (knowledge_base_id, name)
);

CREATE TABLE IF NOT EXISTS wiki_page_tags (
    page_id UUID NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (page_id, tag_id)
);

CREATE TABLE IF NOT EXISTS knowledge_edges (
    id UUID PRIMARY KEY,
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases(id),
    source_page_id UUID REFERENCES wiki_pages(id) ON DELETE CASCADE,
    target_page_id UUID REFERENCES wiki_pages(id) ON DELETE CASCADE,
    relation TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (source_page_id IS NOT NULL OR target_page_id IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS ingestion_jobs (
    id UUID PRIMARY KEY,
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases(id),
    source_document_id UUID REFERENCES source_documents(id),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'completed', 'failed', 'retrying', 'quarantined')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    error_message TEXT,
    next_attempt_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS ingestion_attempts (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES ingestion_jobs(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    error_message TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS processing_failures (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES ingestion_jobs(id) ON DELETE CASCADE,
    error_code TEXT NOT NULL,
    error_message TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS retrieval_audits (
    id UUID PRIMARY KEY,
    knowledge_base_id UUID,
    query TEXT NOT NULL,
    configuration JSONB NOT NULL DEFAULT '{}'::jsonb,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
