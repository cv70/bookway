-- Keep the exact equipment context that produced an ad decision.  A route can
-- expose several action-node equipment choices; delivery receipts must not be
-- replayable across those contexts.
ALTER TABLE ad_delivery_decisions
    ADD COLUMN IF NOT EXISTS scene_equipment TEXT NOT NULL;

CREATE INDEX IF NOT EXISTS idx_ad_delivery_decisions_context
    ON ad_delivery_decisions (route_id, action_node_id, scene_equipment, expires_at);
