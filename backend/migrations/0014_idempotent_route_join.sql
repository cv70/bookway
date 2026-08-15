ALTER TABLE journeys
    ADD COLUMN IF NOT EXISTS source_route_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_journeys_user_source_route
    ON journeys (user_id, source_route_id)
    WHERE source_route_id IS NOT NULL;
