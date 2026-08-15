CREATE TABLE IF NOT EXISTS route_participations (
    route_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    private_journey_id TEXT,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    left_at TIMESTAMPTZ,
    PRIMARY KEY (route_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_route_participations_active_route
    ON route_participations (route_id, joined_at DESC)
    WHERE left_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_route_participations_active_user
    ON route_participations (user_id, joined_at DESC)
    WHERE left_at IS NULL;
