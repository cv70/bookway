-- A reminder is user-visible in the inbox before provider delivery. Provider
-- delivery itself needs a lease and retry state so an outage cannot lose it.
ALTER TABLE reminder_deliveries
    DROP CONSTRAINT IF EXISTS reminder_deliveries_status_check;

ALTER TABLE reminder_deliveries
    ADD CONSTRAINT reminder_deliveries_status_check
        CHECK (status IN ('queued', 'processing', 'dispatched', 'canceled', 'failed'));

ALTER TABLE reminder_deliveries
    ADD COLUMN IF NOT EXISTS attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    ADD COLUMN IF NOT EXISTS available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS locked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS lease_id UUID,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS idx_reminder_deliveries_dispatch_claim
    ON reminder_deliveries (available_at, created_at)
    WHERE status IN ('queued', 'processing');
