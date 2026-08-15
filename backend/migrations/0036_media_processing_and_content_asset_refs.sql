-- Upload completion only proves that bytes reached object storage. A durable
-- processing job performs the final integrity/audit pass before an asset can
-- be referenced by public content.
ALTER TABLE media_assets
    DROP CONSTRAINT IF EXISTS media_assets_status_check;

ALTER TABLE media_assets
    ADD CONSTRAINT media_assets_status_check
    CHECK (status IN ('pending', 'processing', 'ready', 'blocked', 'deleted'));

CREATE TABLE IF NOT EXISTS media_processing_jobs (
    asset_id TEXT PRIMARY KEY REFERENCES media_assets(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    lease_id UUID,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_media_processing_jobs_claim
    ON media_processing_jobs (status, available_at, created_at);

-- `content_media` is the immutable, queryable audit record of which owned
-- Media asset was attached to each content revision. Existing historical rows
-- predate Media ownership enforcement, so the column remains nullable for
-- those rows while all new writes provide it.
ALTER TABLE content_media
    ADD COLUMN IF NOT EXISTS media_asset_id TEXT REFERENCES media_assets(id);

-- Older rows used the default sort order for every attachment. Make their
-- inherited ordering deterministic before enforcing the new revision policy.
WITH ordered_media AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY content_id
            ORDER BY sort_order, created_at, id
        )::INTEGER - 1 AS normalized_sort_order
    FROM content_media
)
UPDATE content_media AS media
SET sort_order = ordered_media.normalized_sort_order
FROM ordered_media
WHERE media.id = ordered_media.id
  AND media.sort_order <> ordered_media.normalized_sort_order;

CREATE UNIQUE INDEX IF NOT EXISTS idx_content_media_content_asset
    ON content_media (content_id, media_asset_id)
    WHERE media_asset_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_content_media_content_order
    ON content_media (content_id, sort_order);
