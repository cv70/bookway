-- A mobile retry must not create a second private action when the first
-- response was lost. The key is supplied only for explicit user creates;
-- materialized recurring occurrences intentionally leave it NULL.
ALTER TABLE actions
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_actions_user_idempotency
    ON actions (user_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
