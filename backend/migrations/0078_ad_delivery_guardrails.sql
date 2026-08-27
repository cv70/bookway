-- Delivery guardrails replace the ad-platform page's local-only demo values.
-- One row per scope; readers fall back to the documented default when a row
-- is absent so an operator mistake can never disable the cap silently.
CREATE TABLE IF NOT EXISTS ad_delivery_guardrails (
    scope TEXT PRIMARY KEY CHECK (scope IN ('user_daily_total')),
    value INTEGER NOT NULL CHECK (value > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO ad_delivery_guardrails (scope, value)
VALUES ('user_daily_total', 8)
ON CONFLICT (scope) DO NOTHING;
