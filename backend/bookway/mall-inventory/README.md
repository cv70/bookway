# Mall Inventory

`mall-inventory` owns available stock and reservation state. Reservations are
idempotent by order ID, time-bound, and atomically confirmed or released.

With `STORAGE_MODE=postgres` and `REDIS_URL` configured, reserve requests first
run an atomic Redis Lua check-and-hold against per-SKU cached stock, then commit
the reservation in PostgreSQL. A failed durable write rolls the Redis hold back;
Redis outages fall back to the PostgreSQL transaction, so the cache is never the
sole inventory authority. Confirm, release and stock updates reconcile the
cache after their durable operation; a stale Redis insufficient result is
rechecked against PostgreSQL before returning an out-of-stock response. Stock
reads use the same cache with per-SKU local single-flight and a short
cross-instance refresh lease, so a cold or expired hot SKU does not stampede
PostgreSQL. Cache keys are bounded by `MALL_INVENTORY_REDIS_CACHE_TTL_SECONDS`
(default 300 seconds).

`ExpireReservations` is an internal, bounded sweep endpoint. Run
`bookway-mall-inventory-sweeper` in production so abandoned reservations free
stock even when no later read or checkout request arrives.

## Merchant stock management

`SetStock` applies an absolute saleable count. It is idempotent by value — no
separate idempotency key is needed — and its conditional update refuses any
result below the currently reserved quantity (`FailedPrecondition`), so a stock
count can touch the reserved floor but never cross it while orders hold
inventory. The gateway exposes this to merchant admins as
`POST /v1/admin/mall/skus/{sku_id}/stock`; because inventory rows carry no
merchant fact by design, the gateway proves SKU ownership against the mall's
`VerifyMerchantSku` before forwarding the mutation, and the write refreshes the
Redis stock cache after the durable commit.

