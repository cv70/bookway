-- search-main owns the public multi-recall cursor. bbs-search continues to
-- own each opaque source cursor stored inside this session state.
CREATE TABLE IF NOT EXISTS search_main_sessions (
    session_id TEXT PRIMARY KEY,
    state JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_search_main_sessions_expiry
    ON search_main_sessions (expires_at);
