-- Search Main keeps rewrites as immutable named versions. The singleton pointer
-- changes atomically, while an in-flight search session retains its own version.
CREATE TABLE IF NOT EXISTS search_query_rewrite_versions (
    version TEXT PRIMARY KEY CHECK (
        version ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$'
    ),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (
        status IN ('draft', 'ready', 'retired')
    ),
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS search_query_rewrite_rules (
    version TEXT NOT NULL REFERENCES search_query_rewrite_versions(version) ON DELETE RESTRICT,
    trigger TEXT NOT NULL CHECK (
        char_length(btrim(trigger)) BETWEEN 1 AND 32
    ),
    expansion_terms TEXT[] NOT NULL CHECK (
        cardinality(expansion_terms) BETWEEN 1 AND 6
    ),
    PRIMARY KEY (version, trigger)
);

CREATE TABLE IF NOT EXISTS search_query_rewrite_active (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    version TEXT NOT NULL REFERENCES search_query_rewrite_versions(version) ON DELETE RESTRICT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO search_query_rewrite_versions (version, status, description)
VALUES (
    'builtin-v1',
    'ready',
    'Initial production-safe Chinese lifestyle vocabulary.'
)
ON CONFLICT (version) DO NOTHING;

INSERT INTO search_query_rewrite_rules (version, trigger, expansion_terms)
VALUES
    ('builtin-v1', '跑步', ARRAY['慢跑', '晨跑', '夜跑']),
    ('builtin-v1', '阅读', ARRAY['读书', '书单', '主题阅读']),
    ('builtin-v1', '睡眠', ARRAY['早睡', '作息', '睡眠修复']),
    ('builtin-v1', '冥想', ARRAY['正念', '呼吸', '静坐']),
    ('builtin-v1', '旅行', ARRAY['徒步', '城市漫游', '出行']),
    ('builtin-v1', '徒步', ARRAY['登山', '步道', '远足'])
ON CONFLICT (version, trigger) DO NOTHING;

INSERT INTO search_query_rewrite_active (singleton, version)
VALUES (true, 'builtin-v1')
ON CONFLICT (singleton) DO NOTHING;

CREATE OR REPLACE FUNCTION activate_search_query_rewrite(target_version TEXT)
RETURNS VOID
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM search_query_rewrite_versions
        WHERE version = target_version AND status = 'ready'
    ) THEN
        RAISE EXCEPTION 'query rewrite version % is not ready', target_version;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM search_query_rewrite_rules
        WHERE version = target_version
    ) THEN
        RAISE EXCEPTION 'query rewrite version % has no rules', target_version;
    END IF;
    INSERT INTO search_query_rewrite_active (singleton, version, updated_at)
    VALUES (true, target_version, now())
    ON CONFLICT (singleton) DO UPDATE
    SET version = EXCLUDED.version, updated_at = EXCLUDED.updated_at;
END;
$$;

ALTER TABLE search_exposures
    ADD COLUMN IF NOT EXISTS query_rewrite_version TEXT NOT NULL DEFAULT 'legacy-unversioned';

CREATE INDEX IF NOT EXISTS idx_search_exposures_rewrite_version_time
    ON search_exposures (query_rewrite_version, created_at DESC);
