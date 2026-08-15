-- Search rewrite versions need durable, privacy-safe quality snapshots before
-- they can be compared or promoted. Metrics never include query text or IDs.
CREATE TABLE IF NOT EXISTS search_evaluation_runs (
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

CREATE INDEX IF NOT EXISTS idx_search_evaluation_runs_created
    ON search_evaluation_runs (created_at DESC);
