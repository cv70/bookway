-- A resolved report that restricts content must outlive the synchronous
-- Gateway handoff. The dispatcher owns the idempotent BBS Link retry.
CREATE TABLE IF NOT EXISTS content_report_restriction_jobs (
    report_id TEXT PRIMARY KEY REFERENCES community_reports(id),
    content_id TEXT NOT NULL,
    delivery_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (delivery_status IN ('pending', 'dispatching', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_until TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_content_report_restriction_jobs_pending
    ON content_report_restriction_jobs (available_at, created_at)
    WHERE delivery_status IN ('pending', 'dispatching');
