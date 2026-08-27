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
The same transaction that marks an order paid also enqueues its
route-attributed `purchase` fact in `purchase_event_outbox` (one row per
order), so attribution can no longer be lost between payment and delivery.
`bookway-outbox-relay` resolves the offer's route at delivery time, ingests
the event into user-event under a deterministic idempotency key, retries
transient failures with backoff, and dead-letters rows whose offer lost its
route attribution.

## Affiliate settlement ledger

Each paid order with an attributed creator gets exactly one affiliate
settlement row (`order_id` is unique). Rows start `eligible`; `SettleAffiliate`
marks a share `settled` and is idempotent on replays.

## Refund path (ReverseAffiliate)

`ReverseAffiliate(order_id)` is the ledger hook for refunds: it flips an
`eligible` share to `reversed`, and replaying an already reversed order returns
that row unchanged. It is not a merchant dashboard action — the gateway does
not expose it — and is meant to be invoked by the future refund money channel
when an order reaches its refund final state. A `settled` share has already
been paid out; clawing funds back is the refund money channel's job, not this
hook's (it fails such reversals with `FailedPrecondition`). `pending` shares do
not occur on current write paths and are likewise not reversible here.

