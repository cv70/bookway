-- Creator-facing appeal history is always scoped by appellant and paginated
-- oldest-first, so this index also supports stable cursor continuation.
CREATE INDEX IF NOT EXISTS idx_content_appeals_appellant_created
    ON content_appeals (appellant_id, created_at, id);
