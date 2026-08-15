-- Preserve the original normalized create request so a retry stays safe even
-- after the user has updated the Journey or completed its initial action.
ALTER TABLE journeys
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

ALTER TABLE journeys
    ADD COLUMN IF NOT EXISTS idempotency_payload JSONB;

CREATE UNIQUE INDEX IF NOT EXISTS idx_journeys_user_idempotency
    ON journeys (user_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
