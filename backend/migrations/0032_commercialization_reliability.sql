-- A provider transaction can settle at most one order. mall-order claims this
-- reference before confirming stock, so retries of the same webhook are safe.
CREATE UNIQUE INDEX IF NOT EXISTS uq_mall_orders_payment_reference
    ON mall_orders (payment_reference)
    WHERE payment_reference IS NOT NULL;
