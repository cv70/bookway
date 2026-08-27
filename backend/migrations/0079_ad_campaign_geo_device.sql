-- Delivery targeting dimensions beyond content affinity: advertisers may
-- scope a campaign to geographic regions and device operating systems.
-- Empty arrays mean unrestricted. Context values travel with every delivery
-- request; a request without a known region/os can only match campaigns whose
-- array is empty (fail-closed: unknown context never serves targeted stock).
ALTER TABLE ad_campaigns
    ADD COLUMN IF NOT EXISTS geo_regions TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS device_os TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_ad_campaigns_targeting
    ON ad_campaigns USING gin (geo_regions, device_os);
