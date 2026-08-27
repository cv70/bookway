# Ad Main

`ad-main` is the serving orchestrator. It never owns campaign configuration or
budget state: it requests candidate recall, ranks candidates and forwards
idempotent client exposure/click receipts to `ad-center`. Every decision is
scoped to an explicit public route action node and scene equipment selection; it
cannot serve a free-floating or cross-equipment placement.

## Optional impression pacing

`AD_MAIN_IMPRESSION_COOLDOWN_MS` enables a per-user minimum interval between
decisions that carried ads (`adpace:{user}` timestamp keys). This is a serving
experience throttle configured by the operator — not a delivery guarantee:
keys may drift and expire, and any Redis outage turns pacing off silently
(fail-open) instead of blocking commerce. Delivery frequency caps remain the
exclusive authority of `ad-center`.
