ALTER TABLE user_events
    ADD COLUMN IF NOT EXISTS negative_feedback_reason TEXT;

ALTER TABLE user_events
    DROP CONSTRAINT IF EXISTS user_events_negative_feedback_reason_check;

ALTER TABLE user_events
    ADD CONSTRAINT user_events_negative_feedback_reason_check CHECK (
        negative_feedback_reason IS NULL
        OR (
            event_type = 'hide'
            AND negative_feedback_reason IN ('not_relevant', 'already_seen', 'low_quality')
        )
    );
