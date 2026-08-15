-- A terminal appeal decision and its author inbox work item are committed in
-- one trust-safety transaction. The dispatcher owns retries and only delivers
-- a restore decision after bbs-link confirms the content is public again.
CREATE TABLE IF NOT EXISTS content_appeal_notification_jobs (
    appeal_id TEXT PRIMARY KEY REFERENCES content_appeals(id),
    user_id TEXT NOT NULL,
    content_id TEXT NOT NULL,
    decision_status TEXT NOT NULL CHECK (decision_status IN ('resolved', 'rejected')),
    action TEXT NOT NULL CHECK (action IN ('no_action', 'restore_content')),
    resolution TEXT NOT NULL,
    delivery_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (delivery_status IN ('pending', 'dispatching', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_until TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_content_appeal_notification_jobs_pending
    ON content_appeal_notification_jobs (available_at, created_at)
    WHERE delivery_status IN ('pending', 'dispatching');
