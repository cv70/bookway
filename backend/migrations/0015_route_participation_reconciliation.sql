CREATE TABLE IF NOT EXISTS route_participation_intents (
    user_id TEXT NOT NULL,
    route_id TEXT NOT NULL,
    private_journey_id TEXT REFERENCES journeys(id),
    desired_active BOOLEAN NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    applied_version BIGINT NOT NULL DEFAULT 0 CHECK (applied_version >= 0 AND applied_version <= version),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_until TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, route_id)
);

CREATE INDEX IF NOT EXISTS idx_route_participation_intents_pending
    ON route_participation_intents (available_at, updated_at)
    WHERE applied_version < version;

ALTER TABLE route_participations
    ADD COLUMN IF NOT EXISTS last_intent_version BIGINT NOT NULL DEFAULT 0
    CHECK (last_intent_version >= 0);
