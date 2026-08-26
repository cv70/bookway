# Mall Order

`mall-order` owns user orders and payment state. It creates a durable pending
order before reserving stock, so retries replay the same reservation ID. Payment
first claims a durable processing state, then confirms stock. Cancellation claims
the pending order state before releasing stock; expiry releases uncommitted stock
before marking the order expired. These transitions prevent a concurrent payment
from turning a committed order into a cancelled or expired one.

The internal payment callback claims a provider payment reference before stock
confirmation; PostgreSQL enforces that a reference settles only one order.
`ExpirePending` is consumed by `bookway-mall-order-expirer`, which turns stale
pending or processing orders into `expired` when inventory has not committed,
and retries inventory release independently of a customer read.

When checkout references a contextual `NodeOffer`, the service revalidates the
offer's saleable SKU and snapshots its creator and calculated commission on the
order. Caller-provided creator IDs or commission amounts are never accepted.
After a payment is confirmed, it emits a stable route-attributed `purchase`
event; an event delivery outage does not change the settled order state.
