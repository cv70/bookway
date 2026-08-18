# Knowledge Catalog

`KnowledgeCatalog.RetrieveRagContext` provides the RAG boundary for a public
route action node. It accepts a user question and returns only RAG-enabled
attachments, their cited resource metadata, bounded excerpts, relevance scores,
and stable `embedding_collections`. Until a vector backend is configured, the
service reports `attachment_lexical_fallback`; callers must use the returned
contexts as evidence rather than treating them as a model-generated answer.

`knowledge-catalog` owns public resource metadata only: books, courses, tools, articles and podcasts. A resource must carry a provider, canonical URL, license, version and citation before it can be published. Private knowledge captures and Journey/Action data remain owned by Growth and are never queried here.

`Search` returns only `published` resources, supports bounded query/topic/kind filters and a stable offset cursor. PostgreSQL is the production repository; memory mode seeds a small official-resource fixture for local development. The service listens on `KNOWLEDGE_CATALOG_ADDR` (default `127.0.0.1:8105`) and requires the normal internal gRPC service token.
