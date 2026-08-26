-- Payment confirmation uses an intermediate state so cancellation and expiry
-- cannot race inventory confirmation and turn a paid order into a cancelled one.
ALTER TABLE mall_orders
    DROP CONSTRAINT IF EXISTS mall_orders_status_check;

ALTER TABLE mall_orders
    ADD CONSTRAINT mall_orders_status_check CHECK (
        status IN (
            'pending_payment',
            'payment_processing',
            'paid',
            'cancelled',
            'expired'
        )
    );
