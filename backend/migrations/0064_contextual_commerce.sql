-- Contextual commerce is keyed by public route/action-node identity.  Product
-- and order services can validate the association without copying route data.
CREATE TABLE IF NOT EXISTS mall_node_offers (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES mall_products(id),
    sku_id TEXT NOT NULL REFERENCES mall_skus(id),
    route_id TEXT NOT NULL,
    action_node_id TEXT NOT NULL,
    scene_equipment TEXT NOT NULL,
    creator_id TEXT NOT NULL,
    commission_bps INTEGER NOT NULL DEFAULT 0 CHECK (commission_bps BETWEEN 0 AND 3000),
    idempotency_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(route_id) > 0 AND length(action_node_id) > 0 AND length(scene_equipment) > 0 AND length(creator_id) > 0)
);

CREATE INDEX IF NOT EXISTS idx_mall_node_offers_lookup
    ON mall_node_offers (route_id, action_node_id, id);

CREATE INDEX IF NOT EXISTS idx_mall_node_offers_creator
    ON mall_node_offers (creator_id, created_at DESC);

ALTER TABLE mall_orders ADD COLUMN IF NOT EXISTS node_offer_id TEXT NOT NULL REFERENCES mall_node_offers(id);
ALTER TABLE mall_orders ADD COLUMN IF NOT EXISTS affiliate_creator_id TEXT NOT NULL;
ALTER TABLE mall_orders ADD COLUMN IF NOT EXISTS commission_cents BIGINT NOT NULL DEFAULT 0 CHECK (commission_cents >= 0);

ALTER TABLE user_events DROP CONSTRAINT IF EXISTS user_events_event_type_check;
ALTER TABLE user_events ADD CONSTRAINT user_events_event_type_check CHECK (
    event_type IN (
        'impression', 'click', 'view', 'like', 'bookmark', 'save_knowledge',
        'share', 'hide', 'complete', 'join_route', 'follow', 'report',
        'search_submit', 'purchase'
    )
);
