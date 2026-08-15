-- Gateway resolves notification recipients while coordinating interactions
-- owned by several services. This queue makes the Growth handoff durable;
-- source_id is also Growth's idempotency key for retry-safe delivery.
CREATE TABLE IF NOT EXISTS community_notification_jobs (
    source_id TEXT PRIMARY KEY,
    recipient_user_id TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    data JSONB NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(data) = 'object'),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    lease_id UUID,
    delivered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_community_notification_jobs_claim
    ON community_notification_jobs (available_at, created_at)
    WHERE status IN ('pending', 'processing');
