CREATE TABLE IF NOT EXISTS user_feature_snapshots (
    snapshot_id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    as_of TIMESTAMPTZ NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    features JSONB NOT NULL,
    lineage JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT user_feature_snapshots_window_check CHECK (window_start < window_end),
    CONSTRAINT user_feature_snapshots_expiry_check CHECK (expires_at > as_of),
    UNIQUE (user_id, feature_version, as_of)
);

CREATE INDEX IF NOT EXISTS idx_user_feature_snapshots_latest
    ON user_feature_snapshots (user_id, feature_version, as_of DESC);

CREATE INDEX IF NOT EXISTS idx_user_feature_snapshots_expiry
    ON user_feature_snapshots (expires_at);
