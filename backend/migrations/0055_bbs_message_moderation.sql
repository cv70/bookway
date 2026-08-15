-- Private-message reporting is intentionally scoped to the recipient. The
-- original message remains in direct_messages and is only joined by trusted
-- moderation RPCs; it is never copied into a public-facing read model.
CREATE TABLE IF NOT EXISTS direct_message_reports (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES direct_messages (id) ON DELETE RESTRICT,
    reporter_user_id TEXT NOT NULL,
    reported_user_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('spam', 'harassment', 'unsafe', 'fraud', 'privacy', 'other')),
    details TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'reviewing', 'resolved', 'rejected')),
    reviewer_user_id TEXT,
    resolution TEXT,
    action TEXT NOT NULL DEFAULT 'no_action' CHECK (action IN ('no_action', 'restrict_sender')),
    idempotency_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (reporter_user_id <> reported_user_id),
    CHECK (char_length(details) <= 1000),
    CHECK (resolution IS NULL OR char_length(resolution) BETWEEN 1 AND 1000),
    CHECK (
        (status = 'pending' AND reviewer_user_id IS NULL AND resolution IS NULL AND action = 'no_action')
        OR (status = 'reviewing' AND reviewer_user_id IS NOT NULL AND resolution IS NULL AND action = 'no_action')
        OR (status = 'resolved' AND reviewer_user_id IS NOT NULL AND resolution IS NOT NULL)
        OR (status = 'rejected' AND reviewer_user_id IS NOT NULL AND resolution IS NOT NULL AND action = 'no_action')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS direct_message_reports_reporter_idempotency_key
    ON direct_message_reports (reporter_user_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_direct_message_reports_queue
    ON direct_message_reports (status, created_at ASC, id ASC);
CREATE INDEX IF NOT EXISTS idx_direct_message_reports_message
    ON direct_message_reports (message_id, created_at ASC, id ASC);

-- Restrictions are permanent until a future explicit appeals/unrestriction
-- workflow is introduced. Keeping the source report makes each restriction
-- auditable and lets writes enforce it with a single indexed lookup.
CREATE TABLE IF NOT EXISTS direct_message_restrictions (
    sender_user_id TEXT PRIMARY KEY,
    report_id TEXT NOT NULL REFERENCES direct_message_reports (id) ON DELETE RESTRICT,
    reviewer_user_id TEXT NOT NULL,
    resolution TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_direct_message_restrictions_report
    ON direct_message_restrictions (report_id);
