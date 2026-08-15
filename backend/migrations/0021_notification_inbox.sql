-- The inbox is the durable, user-visible counterpart to notification delivery.
-- Producers use (kind, source_id) as an idempotency key so retries do not create
-- duplicate notices.
CREATE TABLE IF NOT EXISTS user_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('action_reminder', 'community', 'system')),
    source_id TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    data JSONB NOT NULL DEFAULT '{}',
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (kind, source_id)
);

CREATE INDEX IF NOT EXISTS idx_user_notifications_inbox
    ON user_notifications (user_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_user_notifications_unread
    ON user_notifications (user_id, created_at DESC, id DESC)
    WHERE read_at IS NULL;
