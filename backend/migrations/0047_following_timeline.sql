-- The following timeline reads the newest public rows for a trusted batch of
-- followed authors. This partial index avoids scanning unrelated authors or
-- non-public moderation states as that author set grows.
CREATE INDEX IF NOT EXISTS idx_content_following_timeline
    ON content_items (author_id, created_at DESC, id DESC)
    WHERE status = 'published' AND deleted_at IS NULL;
