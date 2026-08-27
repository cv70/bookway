# Knowledge Catalog

`KnowledgeCatalog.RetrieveRagContext` provides the RAG boundary for a public
route action node. It accepts a user question and returns only RAG-enabled
attachments, their cited resource metadata, bounded excerpts, relevance scores,
and stable `embedding_collections`. Retrieval mode is negotiated server-side:

- When a trusted orchestrator supplies an `embedding_model` and
  `query_embedding`, the node-scoped vector index is used directly.
- Otherwise, if an embedding provider is configured (below), the service
  embeds the question itself and reports `vector` on hits.
- Any provider failure — or a deployment without one — silently degrades to
  `attachment_lexical_fallback`.

The explicit `UpsertRagEmbedding` and `SearchRagEmbeddings` RPCs keep vector
writes and queries inside the same route/action-node tenant boundary. Callers
must use the returned contexts as evidence rather than treating them as a
model-generated answer.

## Vector retrieval provider

Semantic retrieval activates when all of these are set on the service:

| Variable | Meaning |
| --- | --- |
| `RAG_VECTOR_ENABLED` | Optional kill switch; defaults to enabled. |
| `RAG_EMBEDDING_ENDPOINT` | Base URL of any OpenAI-compatible server (`POST {endpoint}/embeddings`). |
| `RAG_EMBEDDING_API_KEY` | Optional bearer token. |
| `RAG_EMBEDDING_MODEL` | Model name stored alongside every vector. |

With no configuration the service behaves exactly as the lexical-only build.
Vector dimensions are bounded to 8..=4096, matching the 0072 migration CHECK.

## Embedding builder job

`cmd/knowledge-embedding-builder` back-fills vectors for every rag-enabled,
non-archived attachment that has no row in `route_node_resource_embeddings`.
It claims work with `FOR UPDATE SKIP LOCKED` plus a lease window, derives the
document text from public metadata plus the creator's note only (mirroring the
domain's `rag_excerpt`), embeds it through the same OpenAI-compatible API, and
writes the result via the protected `UpsertRagEmbedding` RPC. Failures retry
with capped exponential backoff up to ten attempts; the reason stays readable
in `embedding_last_error`. Absence of a stored embedding is the single source
of truth for "pending" — the attempt/lease columns only pace retries.

Configuration: `RAG_EMBEDDING_ENDPOINT`, `RAG_EMBEDDING_API_KEY`,
`RAG_EMBEDDING_MODEL` (required) and `KNOWLEDGE_CATALOG_GRPC_URL`
(default `http://127.0.0.1:8105`).

---

`knowledge-catalog` owns public resource metadata only: books, courses, tools, articles and podcasts. Nodes can attach typed documents, resource packages, tool checklists, action guides or RAG corpora. A resource must carry a provider, canonical URL, license, version and citation before it can be published. Private knowledge captures and Journey/Action data remain owned by Growth and are never queried here.

Gateway exposes `POST /v1/routes/{route_id}/nodes/{action_node_id}/rag-context` for a bounded user question. The public boundary accepts question text only and never transports client-side vectors; whether the question is embedded for semantic retrieval is decided by the catalog service itself. Responses contain cited attachment metadata and excerpts, never a generated answer or uncited resource body.

`Search` returns only `published` resources, supports bounded query/topic/kind filters and a stable offset cursor. PostgreSQL is the production Dao; memory mode seeds a small official-resource fixture for local development. The service listens on `KNOWLEDGE_CATALOG_ADDR` (default `127.0.0.1:8105`) and requires the normal internal gRPC service token.
