-- Conversion events close the ad attribution loop: ad-rank can calibrate
-- serving CVR against observed post-impression conversions instead of
-- inventing one. Conversions are recorded facts only; they are never billed.
ALTER TABLE ad_campaigns
    ADD COLUMN IF NOT EXISTS conversions BIGINT NOT NULL DEFAULT 0;

ALTER TABLE ad_campaign_daily_stats
    ADD COLUMN IF NOT EXISTS conversions BIGINT NOT NULL DEFAULT 0;
