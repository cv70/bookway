-- Keep the existing local-day column for efficient Today queries while storing
-- the exact instant and selected timezone needed by reminders and recovery.
ALTER TABLE actions
    ADD COLUMN IF NOT EXISTS scheduled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS scheduled_timezone TEXT;

CREATE INDEX IF NOT EXISTS idx_actions_user_schedule_exact
    ON actions (user_id, scheduled_for, state, scheduled_at);
