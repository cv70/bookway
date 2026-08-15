ALTER TABLE user_events DROP CONSTRAINT IF EXISTS user_events_event_type_check;
ALTER TABLE user_events ADD CONSTRAINT user_events_event_type_check CHECK (
    event_type IN (
        'impression',
        'click',
        'view',
        'like',
        'bookmark',
        'share',
        'hide',
        'complete',
        'join_route',
        'follow',
        'report',
        'search_submit'
    )
);

CREATE INDEX IF NOT EXISTS idx_user_events_user_content_time
    ON user_events (user_id, content_id, occurred_at DESC)
    WHERE content_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_content_domain_author
    ON content_items (domain, author_id, id)
    WHERE deleted_at IS NULL;
