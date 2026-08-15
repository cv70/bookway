-- A captured community item is a reference to canonical public content, not a
-- copied body. This stable identity deduplicates a user's capture even if the
-- public post is later edited, and the partial index keeps ordinary resources
-- free to share the same absent source value.
ALTER TABLE knowledge_resources
    ADD COLUMN IF NOT EXISTS source_content_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_resources_user_source_content
    ON knowledge_resources (user_id, source_content_id)
    WHERE source_content_id IS NOT NULL;
