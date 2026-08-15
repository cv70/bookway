-- Reminder delivery is deliberately separated from provider delivery. The
-- dispatcher creates a durable, deduplicated command; a provider consumer can
-- resolve the device endpoint only after it has checked the delivery state.
ALTER TABLE actions
    ADD COLUMN IF NOT EXISTS schedule_revision INTEGER NOT NULL DEFAULT 1
        CHECK (schedule_revision > 0);

CREATE TABLE IF NOT EXISTS reminder_preferences (
    user_id TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT false,
    lead_minutes SMALLINT NOT NULL DEFAULT 0 CHECK (lead_minutes BETWEEN 0 AND 1440),
    timezone TEXT NOT NULL,
    quiet_hours_start TIME,
    quiet_hours_end TIME,
    version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((quiet_hours_start IS NULL) = (quiet_hours_end IS NULL)),
    CHECK (quiet_hours_start IS NULL OR quiet_hours_start <> quiet_hours_end)
);

CREATE TABLE IF NOT EXISTS push_devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('expo', 'fcm', 'apns')),
    -- This opaque endpoint is intentionally not exposed through API or events.
    endpoint TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    UNIQUE (device_id)
);
CREATE INDEX IF NOT EXISTS idx_push_devices_active_user
    ON push_devices (user_id, updated_at DESC)
    WHERE active;

CREATE TABLE IF NOT EXISTS reminder_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    action_id TEXT NOT NULL REFERENCES actions(id),
    device_id TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'push' CHECK (channel = 'push'),
    schedule_revision INTEGER NOT NULL CHECK (schedule_revision > 0),
    scheduled_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'dispatched', 'canceled', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    canceled_at TIMESTAMPTZ,
    dispatched_at TIMESTAMPTZ,
    last_error TEXT,
    UNIQUE (action_id, schedule_revision, channel, device_id)
);
CREATE INDEX IF NOT EXISTS idx_reminder_deliveries_provider_claim
    ON reminder_deliveries (status, created_at)
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS idx_actions_reminder_due
    ON actions (scheduled_at, user_id, schedule_revision)
    WHERE state = 'pending' AND scheduled_at IS NOT NULL;
