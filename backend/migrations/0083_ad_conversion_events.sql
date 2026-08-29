-- Conversion events close the ad attribution loop: ad-rank can calibrate
-- serving CVR against observed post-impression conversions instead of
-- inventing one. Conversions are recorded facts only; they are never billed.

-- `0031_commercialization` originally constrained delivery events to
-- impressions and clicks. Replace that constraint before the new event type is
-- used by the payment outbox relay; otherwise PostgreSQL rejects every
-- server-verified conversion even though the application contract accepts it.
ALTER TABLE ad_delivery_events
    DROP CONSTRAINT IF EXISTS ad_delivery_events_event_type_check;

ALTER TABLE ad_delivery_events
    ADD CONSTRAINT ad_delivery_events_event_type_check
    CHECK (event_type IN ('impression', 'click', 'conversion'));

ALTER TABLE ad_campaigns
    ADD COLUMN IF NOT EXISTS conversions BIGINT NOT NULL DEFAULT 0;

ALTER TABLE ad_campaign_daily_stats
    ADD COLUMN IF NOT EXISTS conversions BIGINT NOT NULL DEFAULT 0;
