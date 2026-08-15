-- Reports and appeals retain moderation metadata separately from comments.
-- The comment row remains the source of truth for content and visibility.
CREATE TABLE IF NOT EXISTS comment_reports (
    id TEXT PRIMARY KEY,
    comment_id TEXT NOT NULL REFERENCES comments(id) ON DELETE RESTRICT,
    reporter_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('spam', 'harassment', 'unsafe', 'fraud', 'privacy', 'other')),
    details TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'reviewing', 'resolved', 'rejected')),
    reviewer_id TEXT,
    resolution TEXT,
    action TEXT NOT NULL DEFAULT 'no_action' CHECK (action IN ('no_action', 'restrict_comment')),
    idempotency_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (char_length(details) <= 1000),
    CHECK (resolution IS NULL OR char_length(resolution) BETWEEN 1 AND 1000),
    CHECK (
        (status = 'pending' AND reviewer_id IS NULL AND resolution IS NULL AND action = 'no_action')
        OR (status = 'reviewing' AND reviewer_id IS NOT NULL AND resolution IS NULL AND action = 'no_action')
        OR (status = 'resolved' AND reviewer_id IS NOT NULL AND resolution IS NOT NULL)
        OR (status = 'rejected' AND reviewer_id IS NOT NULL AND resolution IS NOT NULL AND action = 'no_action')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS comment_reports_reporter_idempotency_key
    ON comment_reports (reporter_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_comment_reports_queue
    ON comment_reports (status, created_at ASC, id ASC);
CREATE INDEX IF NOT EXISTS idx_comment_reports_comment
    ON comment_reports (comment_id, created_at ASC, id ASC);

CREATE TABLE IF NOT EXISTS comment_appeals (
    id TEXT PRIMARY KEY,
    comment_id TEXT NOT NULL REFERENCES comments(id) ON DELETE RESTRICT,
    author_id TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'reviewing', 'resolved', 'rejected')),
    reviewer_id TEXT,
    resolution TEXT,
    action TEXT NOT NULL DEFAULT 'no_action' CHECK (action IN ('no_action', 'restore_comment')),
    idempotency_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (char_length(details) <= 1000),
    CHECK (resolution IS NULL OR char_length(resolution) BETWEEN 1 AND 1000),
    CHECK (
        (status = 'pending' AND reviewer_id IS NULL AND resolution IS NULL AND action = 'no_action')
        OR (status = 'reviewing' AND reviewer_id IS NOT NULL AND resolution IS NULL AND action = 'no_action')
        OR (status = 'resolved' AND reviewer_id IS NOT NULL AND resolution IS NOT NULL)
        OR (status = 'rejected' AND reviewer_id IS NOT NULL AND resolution IS NOT NULL AND action = 'no_action')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS comment_appeals_author_idempotency_key
    ON comment_appeals (author_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_comment_appeals_queue
    ON comment_appeals (status, created_at ASC, id ASC);
CREATE INDEX IF NOT EXISTS idx_comment_appeals_author
    ON comment_appeals (author_id, created_at ASC, id ASC);
