CREATE TABLE IF NOT EXISTS search_query_history (
    history_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    query_text TEXT NOT NULL,
    search_type TEXT NOT NULL,
    request_count BIGINT NOT NULL DEFAULT 0,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_search_query_history_user_recent
    ON search_query_history (user_id, last_seen_at DESC);

CREATE TABLE IF NOT EXISTS search_query_stats_users (
    query_hash TEXT NOT NULL REFERENCES search_query_stats(query_hash) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (query_hash, user_id)
);

CREATE INDEX IF NOT EXISTS idx_search_query_stats_users_query
    ON search_query_stats_users (query_hash);
