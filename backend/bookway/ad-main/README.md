# Ad Main

`ad-main` is the serving orchestrator. It never owns campaign configuration or
budget state: it requests candidate recall, ranks candidates and forwards
idempotent client exposure/click receipts to `ad-center`. Every decision is
scoped to an explicit public route action node; it cannot serve a free-floating
placement.
