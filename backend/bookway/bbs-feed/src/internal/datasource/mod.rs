use async_trait::async_trait;
use bookway_api::{ApiResponse, FeedDto, FeedQueryRequest};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RecommendMainClientError {
    #[error("recommend-main request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[async_trait]
pub(crate) trait BbsFeedDataSource: Send + Sync {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, RecommendMainClientError>;
}

pub(crate) struct HttpBbsFeedDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpBbsFeedDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl BbsFeedDataSource for HttpBbsFeedDataSource {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, RecommendMainClientError> {
        Ok(self
            .client
            .get(format!("{}/internal/v1/feed", self.base_url))
            .query(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<FeedDto>>()
            .await?
            .data)
    }
}
