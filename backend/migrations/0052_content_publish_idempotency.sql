-- A publish retry must return the immutable response that completed the
-- moderation transition, not the content's later editable representation.
ALTER TABLE content_idempotency_keys
    ADD COLUMN IF NOT EXISTS response_payload JSONB;
