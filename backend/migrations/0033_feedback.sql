CREATE TABLE IF NOT EXISTS user_feedback (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('bug', 'feature', 'experience', 'content', 'other')),
    content TEXT NOT NULL,
    contact TEXT NOT NULL DEFAULT '',
    platform TEXT NOT NULL DEFAULT '',
    app_version TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'resolved', 'closed')),
    resolution TEXT,
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_user_feedback_owner_history
    ON user_feedback (user_id, updated_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_user_feedback_queue
    ON user_feedback (status, updated_at DESC, id DESC);
