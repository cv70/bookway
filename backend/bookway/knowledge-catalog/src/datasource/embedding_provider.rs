//! Pluggable text-embedding backend for RAG retrieval.
//!
//! Talks to any OpenAI-compatible `POST /embeddings` endpoint. When no
//! endpoint is configured the provider is disabled and every consumer falls
//! back to lexical retrieval — a missing or failing provider must never make
//! a request fail, it only removes ranking signal.

use std::time::Duration;

use serde::Deserialize;

use crate::conf::EmbeddingConfig;

/// Dimensions are bounded by the 0072 migration CHECK
/// (cardinality(embedding) BETWEEN 8 AND 4096).
pub(crate) const EMBEDDING_DIM_RANGE: std::ops::RangeInclusive<usize> = 8..=4096;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub(crate) enum EmbeddingError {
    #[error("embedding request failed: {0}")]
    Request(String),
    #[error("embedding provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("embedding vector dimension {actual} is outside {min}..={max}")]
    Dimension {
        actual: usize,
        min: usize,
        max: usize,
    },
}

#[async_trait::async_trait]
pub(crate) trait EmbeddingProvider: Send + Sync {
    fn model(&self) -> &str;
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;
}

pub(crate) struct OpenAiCompatibleEmbeddingProvider {
    client: reqwest::Client,
    config: EmbeddingConfig,
}

impl OpenAiCompatibleEmbeddingProvider {
    pub(crate) fn new(config: EmbeddingConfig) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            config,
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OpenAiCompatibleEmbeddingProvider {
    fn model(&self) -> &str {
        &self.config.model
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let mut request = self
            .client
            .post(format!("{}/embeddings", self.config.endpoint.trim_end_matches('/')))
            .timeout(HTTP_TIMEOUT)
            .json(&serde_json::json!({
                "model": self.config.model,
                "input": [text],
            }));
        if let Some(api_key) = self.config.api_key.as_deref().filter(|key| !key.is_empty()) {
            request = request.bearer_auth(api_key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| EmbeddingError::Request(error.to_string()))?;
        if !response.status().is_success() {
            return Err(EmbeddingError::Request(format!(
                "provider returned {}",
                response.status()
            )));
        }
        let payload: EmbeddingsResponse = response
            .json()
            .await
            .map_err(|error| EmbeddingError::InvalidResponse(error.to_string()))?;
        let embedding = payload
            .data
            .into_iter()
            .next()
            .ok_or_else(|| {
                EmbeddingError::InvalidResponse("empty embeddings data array".to_string())
            })?
            .embedding;
        validate_embedding(&embedding)?;
        Ok(embedding)
    }
}

fn validate_embedding(embedding: &[f32]) -> Result<(), EmbeddingError> {
    if !EMBEDDING_DIM_RANGE.contains(&embedding.len()) {
        return Err(EmbeddingError::Dimension {
            actual: embedding.len(),
            min: *EMBEDDING_DIM_RANGE.start(),
            max: *EMBEDDING_DIM_RANGE.end(),
        });
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(EmbeddingError::InvalidResponse(
            "embedding contains non-finite values".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingsData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsData {
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::{EmbeddingError, EMBEDDING_DIM_RANGE, validate_embedding};

    #[test]
    fn rejects_vectors_outside_the_persisted_dimension_guardrail() {
        assert!(matches!(
            validate_embedding(&[0.5; 4]),
            Err(EmbeddingError::Dimension { actual: 4, .. })
        ));
        assert!(validate_embedding(&[0.1; 8]).is_ok());
        assert!(validate_embedding(&[0.1; 4096]).is_ok());
        assert!(matches!(
            validate_embedding(&[0.5; 4097]),
            Err(EmbeddingError::Dimension { actual: 4097, .. })
        ));
        assert_eq!((*EMBEDDING_DIM_RANGE.start(), *EMBEDDING_DIM_RANGE.end()), (8, 4096));
    }

    #[test]
    fn rejects_non_finite_embeddings() {
        assert!(matches!(
            validate_embedding(&[f64::NAN as f32; 16]),
            Err(EmbeddingError::InvalidResponse(_))
        ));
    }
}
