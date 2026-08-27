-- Route-attributed purchases are queued for user-event delivery in the same
-- transaction that marks a mall order paid. One row per order; the relay
-- (cmd/outbox-relay) resolves the offer's route and ingests the event with a
-- deterministic idempotency key, so replays never double-count attribution.
CREATE TABLE IF NOT EXISTS purchase_event_outbox (
    order_id TEXT PRIMARY KEY REFERENCES mall_orders(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    node_offer_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_purchase_event_outbox_claim
    ON purchase_event_outbox (available_at, created_at)
    WHERE status IN ('pending', 'processing');

-- Orders already paid before this migration shipped must still be attributed.
-- Rows are enqueued with the order's stored node attribution exactly once;
-- later transitions never rewrite them.
INSERT INTO purchase_event_outbox (order_id, user_id, node_offer_id)
SELECT id, user_id, node_offer_id
FROM mall_orders
WHERE status = 'paid' AND node_offer_id <> ''
ON CONFLICT (order_id) DO NOTHING;
