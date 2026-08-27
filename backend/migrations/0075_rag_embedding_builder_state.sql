-- The RAG embedding builder derives one vector per rag-enabled attachment.
-- Absence of a route_node_resource_embeddings row is the single source of
-- truth for "pending": these columns only carry retry bookkeeping, so state
-- can never drift from the stored embeddings themselves.
ALTER TABLE route_node_resource_attachments
    ADD COLUMN IF NOT EXISTS embedding_attempts INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS embedding_next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS embedding_lease_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS embedding_last_error TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_route_node_resource_embedding_pending
    ON route_node_resource_attachments (embedding_next_attempt_at)
    WHERE archived_at IS NULL AND rag_enabled = TRUE;
