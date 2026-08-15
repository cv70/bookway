-- trust-safety owns author appeals separately from third-party reports. Keeping
-- both facts immutable avoids a later appeal rewriting the original disposition.
CREATE TABLE IF NOT EXISTS content_appeals (
    id TEXT PRIMARY KEY,
    appellant_id TEXT NOT NULL,
    content_id TEXT NOT NULL,
    details TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'reviewing', 'resolved', 'rejected')),
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    assignee_id TEXT,
    resolution TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (appellant_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_content_appeals_queue
    ON content_appeals (status, created_at, id)
    WHERE status IN ('pending', 'reviewing');
CREATE INDEX IF NOT EXISTS idx_content_appeals_content
    ON content_appeals (content_id, created_at DESC);
