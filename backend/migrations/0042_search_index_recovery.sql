-- Search index projection is an eventually consistent derivative of content.
-- Recovery runs retain enough evidence to replay dead jobs without silently
-- discarding their prior failure context.
CREATE TABLE IF NOT EXISTS content_index_recovery_runs (
    id UUID PRIMARY KEY,
    action TEXT NOT NULL CHECK (action IN ('report', 'requeue_dead')),
    actor TEXT CHECK (actor IS NULL OR char_length(actor) <= 128),
    reason TEXT CHECK (reason IS NULL OR char_length(reason) <= 500),
    requested_limit INTEGER NOT NULL CHECK (requested_limit > 0),
    min_dead_age_seconds INTEGER NOT NULL CHECK (min_dead_age_seconds >= 0),
    recovered_count INTEGER NOT NULL DEFAULT 0 CHECK (recovered_count >= 0),
    summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK (
        action = 'report'
        OR (
            actor IS NOT NULL AND char_length(btrim(actor)) > 0
            AND reason IS NOT NULL AND char_length(btrim(reason)) > 0
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_content_index_recovery_runs_created
    ON content_index_recovery_runs (created_at DESC);

CREATE TABLE IF NOT EXISTS content_index_recovery_items (
    run_id UUID NOT NULL REFERENCES content_index_recovery_runs(id) ON DELETE CASCADE,
    content_id TEXT NOT NULL,
    previous_version BIGINT NOT NULL CHECK (previous_version > 0),
    requeued_version BIGINT NOT NULL CHECK (requeued_version > 0),
    previous_attempts INTEGER NOT NULL CHECK (previous_attempts >= 0),
    previous_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, content_id)
);

CREATE INDEX IF NOT EXISTS idx_content_index_recovery_items_content
    ON content_index_recovery_items (content_id, created_at DESC);
