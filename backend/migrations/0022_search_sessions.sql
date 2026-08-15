CREATE TABLE IF NOT EXISTS search_sessions (
    session_id TEXT PRIMARY KEY,
    state JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_search_sessions_expiry
    ON search_sessions (expires_at);
