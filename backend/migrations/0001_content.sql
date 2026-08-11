CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE IF NOT EXISTS content_items (
    id TEXT PRIMARY KEY,
    author_id TEXT NOT NULL,
    content_type TEXT NOT NULL CHECK (content_type IN ('note', 'article', 'video', 'route')),
    status TEXT NOT NULL CHECK (status IN ('draft', 'reviewing', 'published', 'restricted', 'deleted')),
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    domain TEXT NOT NULL,
    cover_url TEXT NOT NULL DEFAULT '',
    route_title TEXT NOT NULL DEFAULT '',
    route_duration TEXT NOT NULL DEFAULT '',
    version BIGINT NOT NULL DEFAULT 1,
    quality_score DOUBLE PRECISION NOT NULL DEFAULT 0,
    moderation_version TEXT,
    moderation_reason TEXT,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);
ALTER TABLE content_items ADD COLUMN IF NOT EXISTS payload JSONB NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_content_publish_feed
    ON content_items (status, published_at DESC, quality_score DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_content_author_status
    ON content_items (author_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS content_media (
    id TEXT PRIMARY KEY,
    content_id TEXT NOT NULL REFERENCES content_items(id),
    object_key TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0,
    duration_ms BIGINT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS content_topics (
    content_id TEXT NOT NULL REFERENCES content_items(id),
    topic_slug TEXT NOT NULL,
    PRIMARY KEY (content_id, topic_slug)
);

CREATE TABLE IF NOT EXISTS content_idempotency_keys (
    user_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    operation TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, idempotency_key, operation)
);
