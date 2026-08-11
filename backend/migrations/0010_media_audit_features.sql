CREATE TABLE IF NOT EXISTS media_assets (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    object_key TEXT NOT NULL UNIQUE,
    bucket TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
    width INTEGER,
    height INTEGER,
    duration_ms BIGINT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'blocked', 'deleted')),
    cdn_url TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_media_owner_status ON media_assets (owner_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS content_audits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('reviewing', 'approved', 'restricted', 'rejected')),
    risk_score DOUBLE PRECISION NOT NULL DEFAULT 0,
    reasons JSONB NOT NULL DEFAULT '[]',
    provider TEXT NOT NULL,
    reviewer_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (content_id, version)
);
CREATE INDEX IF NOT EXISTS idx_content_audits_content ON content_audits (content_id, version DESC);

CREATE TABLE IF NOT EXISTS user_features (
    user_id TEXT NOT NULL,
    feature_name TEXT NOT NULL,
    value JSONB NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, feature_name)
);
CREATE TABLE IF NOT EXISTS model_versions (
    model_name TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    endpoint TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
