CREATE TABLE IF NOT EXISTS user_events (
    event_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (
        event_type IN (
            'impression',
            'click',
            'view',
            'like',
            'bookmark',
            'share',
            'hide',
            'complete',
            'follow',
            'search_submit'
        )
    ),
    session_id TEXT NOT NULL,
    request_id TEXT,
    component_id TEXT NOT NULL,
    content_id TEXT,
    position INTEGER CHECK (position IS NULL OR position >= 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_user_events_user_time
    ON user_events (user_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_events_content_time
    ON user_events (content_id, occurred_at DESC)
    WHERE content_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_user_events_request
    ON user_events (request_id, position)
    WHERE request_id IS NOT NULL;
