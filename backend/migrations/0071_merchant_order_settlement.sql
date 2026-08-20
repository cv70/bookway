-- Merchant operations own a read model of paid contextual orders.  All
-- ownership and commission values are immutable snapshots on the order so a
-- later catalog edit cannot redirect a payout.
ALTER TABLE mall_orders
    ADD COLUMN IF NOT EXISTS merchant_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS fulfillment_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (fulfillment_status IN ('pending', 'processing', 'shipped', 'delivered', 'cancelled')),
    ADD COLUMN IF NOT EXISTS tracking_number TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_mall_orders_merchant
    ON mall_orders (merchant_id, id DESC);

CREATE TABLE IF NOT EXISTS mall_affiliate_settlements (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL UNIQUE REFERENCES mall_orders(id),
    merchant_id TEXT NOT NULL,
    creator_id TEXT NOT NULL,
    amount_cents BIGINT NOT NULL CHECK (amount_cents >= 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'eligible', 'settled', 'reversed')),
    eligible_at TIMESTAMPTZ NOT NULL,
    settled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_mall_affiliate_settlements_merchant
    ON mall_affiliate_settlements (merchant_id, status, id DESC);
