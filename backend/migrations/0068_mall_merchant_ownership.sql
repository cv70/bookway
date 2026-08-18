ALTER TABLE mall_products
    ADD COLUMN merchant_id TEXT NOT NULL;

CREATE INDEX idx_mall_products_merchant
    ON mall_products (merchant_id, id);

ALTER TABLE mall_node_offers
    ADD COLUMN merchant_id TEXT NOT NULL;

CREATE INDEX idx_mall_node_offers_merchant
    ON mall_node_offers (merchant_id, id);
