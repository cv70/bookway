# Mall Order

`mall-order` owns user orders and payment state. It creates a durable pending
order before reserving stock, so retries replay the same reservation ID. Payment
confirms stock first; cancellation/expiry releases stock before changing order state.

The internal payment callback claims a provider payment reference before stock
confirmation; PostgreSQL enforces that a reference settles only one order.
`ExpirePending` is consumed by `bookway-mall-order-expirer`, which turns stale
pending orders into `expired` and retries inventory release independently of a
customer read.
