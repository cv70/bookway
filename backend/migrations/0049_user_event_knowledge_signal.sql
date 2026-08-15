-- `save_knowledge` is emitted by Gateway only after a user captures a public
-- item into their private knowledge library. Keep the database constraint in
-- sync with the typed User Event allowlist and feature/evaluation consumers.
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
            'search_submit'
        )
    );
