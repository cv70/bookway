-- Direct-message delivery must not be coupled to the request-time availability
-- of the user inbox. This outbox is inserted in the same transaction as the
-- message and is replayed by bbs-message's dedicated dispatcher.
CREATE TABLE IF NOT EXISTS direct_message_notification_jobs (
    message_id TEXT PRIMARY KEY REFERENCES direct_messages (id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES direct_conversations (id) ON DELETE CASCADE,
    recipient_user_id TEXT NOT NULL,
    sender_user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    lease_id UUID,
    delivered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (recipient_user_id <> sender_user_id)
);

CREATE INDEX IF NOT EXISTS idx_direct_message_notification_jobs_claim
    ON direct_message_notification_jobs (available_at, created_at)
    WHERE status IN ('pending', 'processing');
