CREATE TABLE IF NOT EXISTS knowledge_resources (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('book', 'article', 'course', 'video', 'link', 'note')),
    status TEXT NOT NULL CHECK (status IN ('inbox', 'active', 'completed', 'archived')),
    title TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    journey_id TEXT REFERENCES journeys(id),
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_resources_user_status
    ON knowledge_resources (user_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_resources_user_kind
    ON knowledge_resources (user_id, kind, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_resources_tags
    ON knowledge_resources USING GIN (tags);
CREATE INDEX IF NOT EXISTS idx_knowledge_resources_title_trgm
    ON knowledge_resources USING GIN (title gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_knowledge_resources_creator_trgm
    ON knowledge_resources USING GIN ((payload->>'creator') gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_knowledge_resources_summary_trgm
    ON knowledge_resources USING GIN ((payload->>'summary') gin_trgm_ops);
