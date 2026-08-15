-- Keep the rank identity next to the immutable served list. Offline evaluation
-- must be able to separate a model fallback from the intended experiment.
ALTER TABLE feed_exposures
    ADD COLUMN IF NOT EXISTS model_version TEXT;

ALTER TABLE feed_exposures
    ADD COLUMN IF NOT EXISTS experiment_bucket TEXT;

CREATE INDEX IF NOT EXISTS idx_feed_exposures_evaluation_window
    ON feed_exposures (created_at)
    WHERE user_id IS NOT NULL AND NOT degraded;

-- User Event retains request IDs only after Recommend Main has validated the
-- user, session, content and position. This index supports that exact join.
CREATE INDEX IF NOT EXISTS idx_user_events_recommendation_attribution
    ON user_events (request_id, user_id, content_id, position, received_at)
    WHERE request_id IS NOT NULL AND content_id IS NOT NULL AND position IS NOT NULL;

CREATE TABLE IF NOT EXISTS recommendation_evaluation_runs (
    id UUID PRIMARY KEY,
    data_start_at TIMESTAMPTZ NOT NULL,
    data_cutoff_at TIMESTAMPTZ NOT NULL,
    label_window_hours INTEGER NOT NULL CHECK (label_window_hours BETWEEN 1 AND 720),
    min_rendered_items BIGINT NOT NULL CHECK (min_rendered_items >= 1),
    status TEXT NOT NULL CHECK (status IN ('ready', 'insufficient_data')),
    metrics JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (data_start_at < data_cutoff_at)
);

CREATE INDEX IF NOT EXISTS idx_recommendation_evaluation_runs_created
    ON recommendation_evaluation_runs (created_at DESC);
