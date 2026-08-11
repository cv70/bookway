CREATE TABLE IF NOT EXISTS search_documents (
    document_id TEXT PRIMARY KEY,
    document_type TEXT NOT NULL,
    source_version BIGINT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    tags TEXT[] NOT NULL DEFAULT '{}',
    topic_slugs TEXT[] NOT NULL DEFAULT '{}',
    domain TEXT,
    author_id TEXT,
    status TEXT NOT NULL,
    search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('simple', coalesce(body, '')), 'B')
    ) STORED,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_search_documents_vector ON search_documents USING GIN (search_vector);
CREATE INDEX IF NOT EXISTS idx_search_documents_title_trgm ON search_documents USING GIN (title gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_search_documents_scope ON search_documents (document_type, status, indexed_at DESC);

CREATE TABLE IF NOT EXISTS search_query_stats (
    query_hash TEXT PRIMARY KEY,
    query_text TEXT NOT NULL,
    search_type TEXT NOT NULL,
    request_count BIGINT NOT NULL DEFAULT 0,
    zero_result_count BIGINT NOT NULL DEFAULT 0,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
