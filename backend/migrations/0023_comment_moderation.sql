CREATE INDEX IF NOT EXISTS idx_comments_public_post_time
    ON comments (post_id, created_at ASC, id ASC)
    WHERE deleted_at IS NULL AND moderation_state = 'published';
