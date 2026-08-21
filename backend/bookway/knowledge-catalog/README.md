# Knowledge Catalog

`KnowledgeCatalog.RetrieveRagContext` provides the RAG boundary for a public
route action node. It accepts a user question and returns only RAG-enabled
attachments, their cited resource metadata, bounded excerpts, relevance scores,
and stable `embedding_collections`. A trusted orchestrator may supply an
`embedding_model` and `query_embedding` to use the node-scoped vector index;
otherwise the service reports `attachment_lexical_fallback`. The explicit
`UpsertRagEmbedding` and `SearchRagEmbeddings` RPCs keep vector writes and
queries inside the same route/action-node tenant boundary. Callers must use the
returned contexts as evidence rather than treating them as a model-generated
answer.

`knowledge-catalog` owns public resource metadata only: books, courses, tools, articles and podcasts. Nodes can attach typed documents, resource packages, tool checklists, action guides or RAG corpora. A resource must carry a provider, canonical URL, license, version and citation before it can be published. Private knowledge captures and Journey/Action data remain owned by Growth and are never queried here.

Gateway exposes `POST /v1/routes/{route_id}/nodes/{action_node_id}/rag-context` for a bounded user question. The public boundary accepts question text only; it always uses the catalog's node-scoped lexical fallback, while trusted internal workers may supply embeddings through the protected vector RPCs. Responses contain cited attachment metadata and excerpts, never a generated answer or uncited resource body.

`Search` returns only `published` resources, supports bounded query/topic/kind filters and a stable offset cursor. PostgreSQL is the production Dao; memory mode seeds a small official-resource fixture for local development. The service listens on `KNOWLEDGE_CATALOG_ADDR` (default `127.0.0.1:8105`) and requires the normal internal gRPC service token.
