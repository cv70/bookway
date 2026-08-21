-- Route completion quality uses a same-user join existence check. Keep the
-- lookup bounded to route adoption events instead of scanning the full event
-- history for every candidate batch.
CREATE INDEX IF NOT EXISTS idx_user_events_route_completion_lookup
    ON user_events (content_id, user_id, event_type, occurred_at DESC)
    WHERE content_id IS NOT NULL
      AND event_type IN ('join_route', 'complete');
