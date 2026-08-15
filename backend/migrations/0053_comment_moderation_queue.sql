CREATE INDEX IF NOT EXISTS idx_comments_reviewing_created
    ON comments (created_at ASC, id ASC)
    WHERE deleted_at IS NULL AND moderation_state = 'reviewing';

CREATE TABLE IF NOT EXISTS comment_moderation_reviews (
    comment_id TEXT PRIMARY KEY REFERENCES comments(id),
    reviewer_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('published', 'restricted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
