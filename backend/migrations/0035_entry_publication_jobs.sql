-- Publishing a private growth entry is intentionally asynchronous: the entry
-- commits first, then this durable job creates and audits the public content.
CREATE TABLE IF NOT EXISTS entry_publication_jobs (
    entry_id TEXT PRIMARY KEY REFERENCES growth_entries(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    content_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    lease_id UUID,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_entry_publication_jobs_claim
    ON entry_publication_jobs (available_at, created_at)
    WHERE status IN ('pending', 'processing');

-- Keep old client-created public records readable without attempting to
-- recreate their already-unknown public post. New records use the job state.
UPDATE growth_entries
SET payload = payload || jsonb_build_object(
    'publication_status',
    CASE WHEN published THEN 3 ELSE 0 END
)
WHERE NOT payload ? 'publication_status';
