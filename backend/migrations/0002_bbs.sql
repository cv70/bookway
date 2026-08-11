CREATE TABLE IF NOT EXISTS social_edges (
    source_user_id TEXT NOT NULL,
    target_user_id TEXT NOT NULL,
    edge_type TEXT NOT NULL CHECK (edge_type IN ('follow', 'block', 'mute')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (source_user_id, target_user_id, edge_type)
);
CREATE INDEX IF NOT EXISTS idx_social_edges_target ON social_edges (target_user_id, edge_type)
    WHERE deleted_at IS NULL;
