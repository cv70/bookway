ALTER TABLE reactions DROP CONSTRAINT IF EXISTS reactions_reaction_type_check;
ALTER TABLE reactions ADD CONSTRAINT reactions_reaction_type_check
    CHECK (reaction_type IN ('like', 'bookmark', 'hide'));

CREATE INDEX IF NOT EXISTS idx_reactions_user_feedback
    ON reactions (user_id, reaction_type, created_at DESC)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS growth_entries (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    journey_id TEXT REFERENCES journeys(id),
    action_id TEXT REFERENCES actions(id),
    payload JSONB NOT NULL,
    published BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_growth_entries_user_time
    ON growth_entries (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_growth_entries_journey_time
    ON growth_entries (journey_id, created_at DESC)
    WHERE journey_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_search_query_stats_text_trgm
    ON search_query_stats USING GIN (query_text gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_search_query_stats_recent
    ON search_query_stats (last_seen_at DESC, request_count DESC);

CREATE TABLE IF NOT EXISTS community_reports (
    id TEXT PRIMARY KEY,
    reporter_id TEXT NOT NULL,
    content_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('spam', 'harassment', 'unsafe', 'misinformation', 'copyright', 'privacy', 'other')),
    details TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'reviewing', 'resolved', 'rejected')),
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    assignee_id TEXT,
    resolution TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (reporter_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_community_reports_queue
    ON community_reports (status, created_at)
    WHERE status IN ('pending', 'reviewing');
CREATE INDEX IF NOT EXISTS idx_community_reports_content
    ON community_reports (content_id, created_at DESC);
