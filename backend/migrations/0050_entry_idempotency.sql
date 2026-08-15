-- A lost CreateEntry response must not create a second private reflection or
-- a second durable public-publication job when the client retries.
ALTER TABLE growth_entries
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_growth_entries_user_idempotency
    ON growth_entries (user_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
