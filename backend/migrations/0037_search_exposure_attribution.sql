-- Search Main owns a short, immutable record of each page it served. User Event
-- validates interactions through Search Main instead of accessing these tables.
CREATE TABLE IF NOT EXISTS search_exposures (
    request_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    query_hash TEXT NOT NULL,
    result_count INTEGER NOT NULL DEFAULT 0,
    degraded BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + interval '30 days')
);

CREATE TABLE IF NOT EXISTS search_exposure_items (
    request_id TEXT NOT NULL REFERENCES search_exposures(request_id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    result_id TEXT NOT NULL,
    result_type TEXT NOT NULL,
    PRIMARY KEY (request_id, position)
);

CREATE INDEX IF NOT EXISTS idx_search_exposures_user_session_time
    ON search_exposures (user_id, session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_search_exposures_expiry
    ON search_exposures (expires_at);
