use async_trait::async_trait;
use bookway_api::{ApiResponse, ErrorResponse};
use serde::de::DeserializeOwned;
use thiserror::Error;

use super::api::{SearchQueryRequest, SearchResponseDto, SuggestionResponseDto};

#[derive(Debug, Error)]
pub(crate) enum SearchClientError {
    #[error("bbs-search request failed: {0}")]
    Transport(#[source] reqwest::Error),
    #[error("bbs-search rejected the request ({status}): {code}: {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
}

#[async_trait]
pub(crate) trait SearchDataSource: Send + Sync {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchClientError>;
    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, SearchClientError>;
}

pub(crate) struct HttpSearchDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpSearchDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl SearchDataSource for HttpSearchDataSource {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchClientError> {
        decode(
            self.client
                .get(format!("{}/internal/v1/search", self.base_url))
                .query(&request)
                .send()
                .await
                .map_err(SearchClientError::Transport)?,
        )
        .await
    }

    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, SearchClientError> {
        decode(
            self.client
                .get(format!("{}/internal/v1/suggestions", self.base_url))
                .query(&[("q", query)])
                .send()
                .await
                .map_err(SearchClientError::Transport)?,
        )
        .await
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, SearchClientError> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<ApiResponse<T>>()
            .await
            .map(|response| response.data)
            .map_err(SearchClientError::Transport);
    }

    let status = status.as_u16();
    match response.json::<ErrorResponse>().await {
        Ok(error) => Err(SearchClientError::Rejected {
            status,
            code: error.error.code,
            message: error.error.message,
        }),
        Err(source) => Err(SearchClientError::Transport(source)),
    }
}
