-- Reconciliation is observational, but durable run state makes an interrupted
-- full audit restartable without exposing its content-ID checkpoint in normal
-- job output.
CREATE TABLE IF NOT EXISTS content_index_reconciliation_runs (
    id UUID PRIMARY KEY,
    target_index TEXT NOT NULL CHECK (char_length(target_index) BETWEEN 1 AND 255),
    status TEXT NOT NULL CHECK (status IN ('running', 'failed', 'completed')),
    full_scan BOOLEAN NOT NULL,
    batch_size INTEGER NOT NULL CHECK (batch_size > 0),
    lease_seconds INTEGER NOT NULL CHECK (lease_seconds > 0),
    next_after_id TEXT NOT NULL DEFAULT '',
    scanned BIGINT NOT NULL DEFAULT 0 CHECK (scanned >= 0),
    expected_public BIGINT NOT NULL DEFAULT 0 CHECK (expected_public >= 0),
    expected_absent BIGINT NOT NULL DEFAULT 0 CHECK (expected_absent >= 0),
    missing BIGINT NOT NULL DEFAULT 0 CHECK (missing >= 0),
    stale BIGINT NOT NULL DEFAULT 0 CHECK (stale >= 0),
    unexpected_present BIGINT NOT NULL DEFAULT 0 CHECK (unexpected_present >= 0),
    source_count BIGINT CHECK (source_count >= 0),
    target_count BIGINT CHECK (target_count >= 0),
    outbox_pending BIGINT NOT NULL DEFAULT 0 CHECK (outbox_pending >= 0),
    outbox_processing BIGINT NOT NULL DEFAULT 0 CHECK (outbox_processing >= 0),
    outbox_dead BIGINT NOT NULL DEFAULT 0 CHECK (outbox_dead >= 0),
    healthy BOOLEAN,
    last_error TEXT CHECK (last_error IS NULL OR char_length(last_error) <= 2000),
    lease_id UUID,
    locked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK (
        (status = 'completed' AND completed_at IS NOT NULL AND healthy IS NOT NULL)
        OR (status <> 'completed' AND completed_at IS NULL AND healthy IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_content_index_reconciliation_runs_status_updated
    ON content_index_reconciliation_runs (status, updated_at DESC);
