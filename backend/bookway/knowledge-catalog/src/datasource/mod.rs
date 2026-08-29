#[path = "embedding_provider.rs"]
mod embedding_provider;
pub(crate) use embedding_provider::{
    EMBEDDING_DIM_RANGE, EmbeddingProvider, OpenAiCompatibleEmbeddingProvider,
};
// Production callers propagate embedding failures without naming the type; the
// name is only needed by the test doubles that implement the trait.
#[cfg(test)]
pub(crate) use embedding_provider::EmbeddingError;
mod support;
pub(crate) use support::*;
