CREATE TABLE IF NOT EXISTS feed_exposures (
    request_id TEXT PRIMARY KEY,
    user_id TEXT,
    session_id TEXT NOT NULL,
    surface TEXT NOT NULL,
    pipeline_id TEXT NOT NULL,
    candidate_count INTEGER NOT NULL DEFAULT 0,
    selected_count INTEGER NOT NULL DEFAULT 0,
    degraded BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS feed_exposure_items (
    request_id TEXT NOT NULL REFERENCES feed_exposures(request_id),
    position INTEGER NOT NULL,
    content_id TEXT NOT NULL,
    source TEXT NOT NULL,
    score DOUBLE PRECISION NOT NULL,
    reasons JSONB NOT NULL DEFAULT '[]',
    PRIMARY KEY (request_id, position)
);
CREATE INDEX IF NOT EXISTS idx_feed_exposure_user_time ON feed_exposures (user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS recommendation_events (
    event_id TEXT PRIMARY KEY,
    request_id TEXT,
    user_id TEXT,
    content_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('impression', 'open', 'like', 'bookmark', 'share', 'hide', 'complete')),
    position INTEGER,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    client_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_recommendation_events_user_time ON recommendation_events (user_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_recommendation_events_content_time ON recommendation_events (content_id, occurred_at DESC);
