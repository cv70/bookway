CREATE TABLE IF NOT EXISTS comments (
    id TEXT PRIMARY KEY,
    post_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    parent_id TEXT REFERENCES comments(id),
    body TEXT NOT NULL,
    moderation_state TEXT NOT NULL DEFAULT 'reviewing',
    like_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_comments_post_time ON comments (post_id, created_at DESC)
    WHERE deleted_at IS NULL;
