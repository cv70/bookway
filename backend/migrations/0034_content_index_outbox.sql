-- Content is the source of truth. Each mutation queues exactly the latest
-- document version for the search projection in the same transaction.
CREATE TABLE IF NOT EXISTS content_index_outbox (
    content_id TEXT PRIMARY KEY REFERENCES content_items(id) ON DELETE CASCADE,
    content_version BIGINT NOT NULL CHECK (content_version > 0),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    lease_id UUID,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_content_index_outbox_claim
    ON content_index_outbox (available_at, created_at)
    WHERE status IN ('pending', 'processing');

-- Existing content must be projected after the migration as well. This makes
-- index rebuilds and first deployment of the Outbox independent of a prior
-- polling cursor.
INSERT INTO content_index_outbox (content_id, content_version)
SELECT id, version
FROM content_items
ON CONFLICT (content_id) DO UPDATE
SET content_version = GREATEST(
        content_index_outbox.content_version,
        EXCLUDED.content_version
    ),
    status = CASE
        WHEN content_index_outbox.status = 'processing' THEN 'processing'
        ELSE 'pending'
    END,
    available_at = now(),
    updated_at = now();
