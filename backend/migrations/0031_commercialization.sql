-- Advertising: campaign configuration and delivery/billing facts are owned by
-- ad-center. Daily stats keep budget enforcement separate from immutable events.
CREATE TABLE IF NOT EXISTS ad_campaigns (
    id TEXT PRIMARY KEY,
    advertiser_id TEXT NOT NULL,
    name TEXT NOT NULL,
    placement TEXT NOT NULL,
    route_id TEXT NOT NULL,
    action_node_id TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    image_url TEXT NOT NULL DEFAULT '',
    landing_url TEXT NOT NULL,
    target_domains JSONB NOT NULL DEFAULT '[]',
    status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'paused', 'ended')),
    pricing_model TEXT NOT NULL CHECK (pricing_model IN ('cpm', 'cpc')),
    bid_micros BIGINT NOT NULL CHECK (bid_micros >= 0),
    daily_budget_micros BIGINT NOT NULL CHECK (daily_budget_micros >= 0),
    frequency_cap INTEGER NOT NULL DEFAULT 0 CHECK (frequency_cap >= 0),
    impressions BIGINT NOT NULL DEFAULT 0 CHECK (impressions >= 0),
    clicks BIGINT NOT NULL DEFAULT 0 CHECK (clicks >= 0),
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (ends_at IS NULL OR starts_at IS NULL OR ends_at > starts_at)
);
CREATE INDEX IF NOT EXISTS idx_ad_campaigns_eligible
    ON ad_campaigns (placement, route_id, action_node_id, status, starts_at, ends_at, bid_micros DESC);

CREATE TABLE IF NOT EXISTS ad_campaign_daily_stats (
    campaign_id TEXT NOT NULL REFERENCES ad_campaigns(id),
    stat_date DATE NOT NULL,
    spent_micros BIGINT NOT NULL DEFAULT 0 CHECK (spent_micros >= 0),
    impressions BIGINT NOT NULL DEFAULT 0 CHECK (impressions >= 0),
    clicks BIGINT NOT NULL DEFAULT 0 CHECK (clicks >= 0),
    PRIMARY KEY (campaign_id, stat_date)
);

CREATE TABLE IF NOT EXISTS ad_delivery_events (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    campaign_id TEXT NOT NULL REFERENCES ad_campaigns(id),
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('impression', 'click')),
    cost_micros BIGINT NOT NULL DEFAULT 0 CHECK (cost_micros >= 0),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_ad_delivery_events_decision_event
    ON ad_delivery_events (request_id, campaign_id, user_id, event_type);
CREATE INDEX IF NOT EXISTS idx_ad_delivery_events_frequency
    ON ad_delivery_events (campaign_id, user_id, event_type, occurred_at DESC);

-- A receipt is accepted only for an opaque decision actually returned to the
-- same user. This prevents arbitrary client requests from consuming budgets.
CREATE TABLE IF NOT EXISTS ad_delivery_decisions (
    request_id TEXT NOT NULL,
    campaign_id TEXT NOT NULL REFERENCES ad_campaigns(id),
    user_id TEXT NOT NULL,
    placement TEXT NOT NULL,
    route_id TEXT NOT NULL,
    action_node_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (request_id, campaign_id)
);
CREATE INDEX IF NOT EXISTS idx_ad_delivery_decisions_expiry
    ON ad_delivery_decisions (expires_at);

-- Catalog is owned by mall; stock and reservations are owned by mall-inventory.
CREATE TABLE IF NOT EXISTS mall_products (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    image_url TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_mall_products_active ON mall_products (status, id);

CREATE TABLE IF NOT EXISTS mall_skus (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES mall_products(id),
    title TEXT NOT NULL,
    price_cents BIGINT NOT NULL CHECK (price_cents >= 0),
    currency TEXT NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}',
    saleable BOOLEAN NOT NULL DEFAULT true
);
CREATE INDEX IF NOT EXISTS idx_mall_skus_product ON mall_skus (product_id, id);

CREATE TABLE IF NOT EXISTS mall_inventory_stock (
    sku_id TEXT PRIMARY KEY,
    available BIGINT NOT NULL CHECK (available >= 0),
    reserved BIGINT NOT NULL DEFAULT 0 CHECK (reserved >= 0 AND reserved <= available),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS mall_inventory_reservations (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'committed', 'released', 'expired')),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_mall_inventory_reservation_expiry
    ON mall_inventory_reservations (status, expires_at);

CREATE TABLE IF NOT EXISTS mall_inventory_reservation_items (
    reservation_id TEXT NOT NULL REFERENCES mall_inventory_reservations(id),
    sku_id TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    PRIMARY KEY (reservation_id, sku_id)
);

-- Orders own payment lifecycle and immutable product/price snapshots.
CREATE TABLE IF NOT EXISTS mall_orders (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending_payment', 'paid', 'cancelled', 'expired')),
    currency TEXT NOT NULL,
    total_cents BIGINT NOT NULL CHECK (total_cents >= 0),
    payment_reference TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_mall_orders_user ON mall_orders (user_id, id DESC);
CREATE INDEX IF NOT EXISTS idx_mall_orders_expiry ON mall_orders (status, expires_at);

CREATE TABLE IF NOT EXISTS mall_order_items (
    order_id TEXT NOT NULL REFERENCES mall_orders(id),
    sku_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    title TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    unit_price_cents BIGINT NOT NULL CHECK (unit_price_cents >= 0),
    currency TEXT NOT NULL,
    line_total_cents BIGINT NOT NULL CHECK (line_total_cents >= 0),
    PRIMARY KEY (order_id, sku_id)
);
