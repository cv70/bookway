-- Node-scoped RAG vectors are derived only from public resource metadata and
-- the creator's attachment note.  Raw external resource bodies never enter
-- this service's database.
CREATE TABLE IF NOT EXISTS route_node_resource_embeddings (
    attachment_id TEXT PRIMARY KEY REFERENCES route_node_resource_attachments(id) ON DELETE CASCADE,
    route_id TEXT NOT NULL,
    action_node_id TEXT NOT NULL,
    embedding_collection TEXT NOT NULL,
    embedding_model TEXT NOT NULL,
    embedding REAL[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (route_id <> ''),
    CHECK (action_node_id <> ''),
    CHECK (embedding_collection <> ''),
    CHECK (embedding_model <> ''),
    CHECK (cardinality(embedding) BETWEEN 8 AND 4096)
);

CREATE INDEX IF NOT EXISTS idx_route_node_resource_embeddings_scope
    ON route_node_resource_embeddings
        (route_id, action_node_id, embedding_collection, embedding_model);
