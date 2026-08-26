-- Campaign model outputs are bounded serving inputs.  They are kept with the
-- campaign snapshot so ad-rank can calculate expected value without another
-- synchronous feature dependency.
ALTER TABLE ad_campaigns
    ADD COLUMN IF NOT EXISTS predicted_ctr DOUBLE PRECISION NOT NULL
        CHECK (predicted_ctr >= 0 AND predicted_ctr <= 1),
    ADD COLUMN IF NOT EXISTS predicted_cvr DOUBLE PRECISION NOT NULL
        CHECK (predicted_cvr >= 0 AND predicted_cvr <= 1),
    ADD COLUMN IF NOT EXISTS global_frequency_cap INTEGER NOT NULL
        CHECK (global_frequency_cap >= 0);

CREATE INDEX IF NOT EXISTS idx_ad_delivery_events_campaign_day
    ON ad_delivery_events (campaign_id, event_type, occurred_at DESC);
