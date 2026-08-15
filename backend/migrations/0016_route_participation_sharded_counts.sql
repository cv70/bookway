CREATE TABLE IF NOT EXISTS route_participation_count_shards (
    route_id TEXT NOT NULL,
    shard_id SMALLINT NOT NULL CHECK (shard_id BETWEEN 0 AND 63),
    active_count BIGINT NOT NULL DEFAULT 0 CHECK (active_count >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (route_id, shard_id)
);

CREATE OR REPLACE FUNCTION bookway_route_participation_shard(participant_user_id TEXT)
RETURNS SMALLINT
LANGUAGE SQL
IMMUTABLE
STRICT
PARALLEL SAFE
AS $$
    SELECT (get_byte(decode(md5(participant_user_id), 'hex'), 0) % 64)::SMALLINT;
$$;

CREATE OR REPLACE FUNCTION bookway_update_route_participation_count_shard()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    affected_rows INTEGER;
    old_shard SMALLINT;
    new_shard SMALLINT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        old_shard := bookway_route_participation_shard(OLD.user_id);
        IF OLD.left_at IS NULL THEN
            UPDATE route_participation_count_shards
            SET active_count = active_count - 1,
                updated_at = now()
            WHERE route_id = OLD.route_id
              AND shard_id = old_shard
              AND active_count > 0;
            GET DIAGNOSTICS affected_rows = ROW_COUNT;
            IF affected_rows <> 1 THEN
                RAISE EXCEPTION 'route participation counter drift for route %, shard %',
                    OLD.route_id,
                    old_shard
                    USING ERRCODE = 'check_violation';
            END IF;
        END IF;
        RETURN OLD;
    END IF;

    new_shard := bookway_route_participation_shard(NEW.user_id);

    IF TG_OP = 'INSERT' THEN
        IF NEW.left_at IS NULL THEN
            INSERT INTO route_participation_count_shards (route_id, shard_id, active_count)
            VALUES (NEW.route_id, new_shard, 1)
            ON CONFLICT (route_id, shard_id) DO UPDATE
            SET active_count = route_participation_count_shards.active_count + 1,
                updated_at = now();
        END IF;
        RETURN NEW;
    END IF;

    old_shard := bookway_route_participation_shard(OLD.user_id);
    IF OLD.left_at IS NULL
       AND (
           NEW.left_at IS NOT NULL
           OR OLD.route_id IS DISTINCT FROM NEW.route_id
           OR OLD.user_id IS DISTINCT FROM NEW.user_id
       ) THEN
        UPDATE route_participation_count_shards
        SET active_count = active_count - 1,
            updated_at = now()
        WHERE route_id = OLD.route_id
          AND shard_id = old_shard
          AND active_count > 0;
        GET DIAGNOSTICS affected_rows = ROW_COUNT;
        IF affected_rows <> 1 THEN
            RAISE EXCEPTION 'route participation counter drift for route %, shard %',
                OLD.route_id,
                old_shard
                USING ERRCODE = 'check_violation';
        END IF;
    END IF;

    IF NEW.left_at IS NULL
       AND (OLD.left_at IS NOT NULL
            OR OLD.route_id IS DISTINCT FROM NEW.route_id
            OR OLD.user_id IS DISTINCT FROM NEW.user_id) THEN
        INSERT INTO route_participation_count_shards (route_id, shard_id, active_count)
        VALUES (NEW.route_id, new_shard, 1)
        ON CONFLICT (route_id, shard_id) DO UPDATE
        SET active_count = route_participation_count_shards.active_count + 1,
            updated_at = now();
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS route_participation_count_shard_trigger ON route_participations;
CREATE TRIGGER route_participation_count_shard_trigger
AFTER INSERT OR UPDATE OF route_id, user_id, left_at OR DELETE
ON route_participations
FOR EACH ROW
EXECUTE FUNCTION bookway_update_route_participation_count_shard();

INSERT INTO route_participation_count_shards (route_id, shard_id, active_count)
SELECT
    route_id,
    bookway_route_participation_shard(user_id) AS shard_id,
    COUNT(*) AS active_count
FROM route_participations
WHERE left_at IS NULL
GROUP BY route_id, shard_id
ON CONFLICT (route_id, shard_id) DO UPDATE
SET active_count = EXCLUDED.active_count,
    updated_at = now();
