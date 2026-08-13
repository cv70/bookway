use async_trait::async_trait;
use bookway_bbs_search::api::pb::{self, bbs_search_client::BbsSearchClient};
use thiserror::Error;

use super::api::{SearchQueryRequest, SearchResponseDto, SuggestionResponseDto};

#[derive(Debug, Error)]
pub(crate) enum SearchClientError {
    #[error("bbs-search request failed: {0}")]
    Transport(String),
}

#[async_trait]
pub(crate) trait SearchDataSource: Send + Sync {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchClientError>;
    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, SearchClientError>;
}

pub(crate) struct GrpcSearchDataSource {
    client: BbsSearchClient<tonic::transport::Channel>,
}

impl GrpcSearchDataSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: BbsSearchClient::connect(base_url).await?,
        })
    }
}

#[async_trait]
impl SearchDataSource for GrpcSearchDataSource {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchClientError> {
        let mut client = self.client.clone();
        let response = client
            .search(pb::SearchRequest {
                request_json: serde_json::to_string(&request)
                    .map_err(|error| SearchClientError::Transport(error.to_string()))?,
            })
            .await
            .map_err(|error| SearchClientError::Transport(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| SearchClientError::Transport(error.to_string()))
    }

    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, SearchClientError> {
        let mut client = self.client.clone();
        let response = client
            .suggestions(pb::SuggestionsRequest {
                query: query.to_string(),
            })
            .await
            .map_err(|error| SearchClientError::Transport(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| SearchClientError::Transport(error.to_string()))
    }
}
