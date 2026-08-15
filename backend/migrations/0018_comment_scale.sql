ALTER TABLE comments
    ADD COLUMN IF NOT EXISTS client_request_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_comments_author_request
    ON comments (author_id, client_request_id)
    WHERE client_request_id IS NOT NULL;

DROP INDEX IF EXISTS idx_comments_post_time;
CREATE INDEX IF NOT EXISTS idx_comments_post_time
    ON comments (post_id, created_at ASC, id ASC)
    WHERE deleted_at IS NULL;
