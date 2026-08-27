use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct EmbeddingConfig {
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) model: String,
}

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
    /// Server-side semantic retrieval is negotiated here, not by callers:
    /// configured provider + `RAG_VECTOR_ENABLED` decide whether questions
    /// are embedded before vector search. Unset means lexical-only, which
    /// stays fully functional.
    pub(crate) embeddings: Option<EmbeddingConfig>,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        let embedding_enabled = std::env::var("RAG_VECTOR_ENABLED")
            .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
            .unwrap_or(true);
        let endpoint = std::env::var("RAG_EMBEDDING_ENDPOINT").unwrap_or_default();
        let model = std::env::var("RAG_EMBEDDING_MODEL").unwrap_or_default();
        let embeddings = if embedding_enabled && !endpoint.trim().is_empty() && !model.is_empty() {
            Some(EmbeddingConfig {
                endpoint,
                api_key: std::env::var("RAG_EMBEDDING_API_KEY").ok().filter(|key| !key.is_empty()),
                model,
            })
        } else {
            None
        };
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("KNOWLEDGE_CATALOG_ADDR", "127.0.0.1:8105")?,
            bbs_link_url: std::env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            embeddings,
        })
    }
}
