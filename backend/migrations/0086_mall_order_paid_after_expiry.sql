-- Reconciliation for payments that arrive after the payment TTL: the provider
-- webhook currently hits an `expired` order and retries failed_precondition
-- forever while holding the buyer's money. The distinct `paid_after_expiry`
-- state records the durable fact that money moved; fulfillment and affiliate
-- settlement are deliberately NOT started from it — operations decides
-- refund vs. fulfill per order.
ALTER TABLE mall_orders
    DROP CONSTRAINT IF EXISTS mall_orders_status_check;

ALTER TABLE mall_orders
    ADD CONSTRAINT mall_orders_status_check CHECK (
        status IN (
            'pending_payment',
            'payment_processing',
            'paid',
            'paid_after_expiry',
            'cancelled',
            'expired'
        )
    );
