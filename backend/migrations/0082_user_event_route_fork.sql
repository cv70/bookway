-- `route_fork` is emitted by Gateway only when a user forks a published
-- route, creating their own editable public copy. Keep the database
-- constraint in sync with the typed User Event allowlist and the
-- feature/evaluation consumers (pattern of 0049/0064).
ALTER TABLE user_events
    DROP CONSTRAINT IF EXISTS user_events_event_type_check;

ALTER TABLE user_events
    ADD CONSTRAINT user_events_event_type_check CHECK (
        event_type IN (
            'impression',
            'click',
            'view',
            'like',
            'bookmark',
            'save_knowledge',
            'share',
            'hide',
            'complete',
            'join_route',
            'follow',
            'report',
            'search_submit',
            'purchase',
            'route_fork'
        )
    );
