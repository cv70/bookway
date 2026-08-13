use async_trait::async_trait;
use bookway_api::{FeedDto, FeedQueryRequest};
use bookway_recommend_main::api::pb::{self, recommend_main_client::RecommendMainClient};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RecommendMainClientError {
    #[error("recommend-main grpc request failed: {0}")]
    Grpc(#[from] tonic::Status),
}

#[async_trait]
pub(crate) trait BbsFeedDataSource: Send + Sync {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, RecommendMainClientError>;
}

#[derive(Clone)]
pub(crate) struct GrpcRecommendMainDataSource {
    client: RecommendMainClient<tonic::transport::Channel>,
}

impl GrpcRecommendMainDataSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: RecommendMainClient::connect(base_url).await?,
        })
    }
}

#[async_trait]
impl BbsFeedDataSource for GrpcRecommendMainDataSource {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, RecommendMainClientError> {
        let mut client = self.client.clone();
        let response = client
            .feed(pb::FeedRequest {
                request_json: serde_json::to_string(&request).map_err(|error| {
                    RecommendMainClientError::Grpc(tonic::Status::internal(error.to_string()))
                })?,
            })
            .await?
            .into_inner();
        serde_json::from_str(&response.response_json).map_err(|error| {
            RecommendMainClientError::Grpc(tonic::Status::internal(error.to_string()))
        })
    }
}
