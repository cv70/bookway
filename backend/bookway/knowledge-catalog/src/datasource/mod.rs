#[path = "embedding_provider.rs"]
mod embedding_provider;
pub(crate) use embedding_provider::{
    EMBEDDING_DIM_RANGE, EmbeddingError, EmbeddingProvider, OpenAiCompatibleEmbeddingProvider,
};
mod support;
pub(crate) use support::*;
