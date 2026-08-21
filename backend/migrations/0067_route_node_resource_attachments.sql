CREATE TABLE IF NOT EXISTS route_node_resource_attachments (
    id TEXT PRIMARY KEY,
    route_id TEXT NOT NULL,
    action_node_id TEXT NOT NULL,
    resource_id TEXT NOT NULL REFERENCES public_resources(id),
    kind TEXT NOT NULL CHECK (
        kind IN (
            'document',
            'pdf',
            'external_link',
            'tool_checklist',
            'ai_action_guide',
            'rag_corpus',
            'resource_package'
        )
    ),
    title_override TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    sort_rank INTEGER NOT NULL DEFAULT 0 CHECK (sort_rank BETWEEN -10000 AND 10000),
    rag_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    embedding_collection TEXT NOT NULL DEFAULT '',
    retrieval_scope TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at TIMESTAMPTZ,
    CHECK (route_id <> ''),
    CHECK (action_node_id <> ''),
    CHECK (created_by <> ''),
    CHECK (
        (rag_enabled = FALSE AND embedding_collection = '')
        OR (rag_enabled = TRUE AND embedding_collection <> '')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_route_node_resource_active_unique
    ON route_node_resource_attachments (route_id, action_node_id, resource_id)
    WHERE archived_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_route_node_resource_active_order
    ON route_node_resource_attachments (route_id, action_node_id, sort_rank, created_at, id)
    WHERE archived_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_route_node_resource_rag_collection
    ON route_node_resource_attachments (embedding_collection, route_id, action_node_id)
    WHERE archived_at IS NULL AND rag_enabled = TRUE;
