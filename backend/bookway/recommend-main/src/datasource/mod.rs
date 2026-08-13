use std::sync::Arc;

use async_trait::async_trait;
use bookway_api::{ReactionContextDto, SocialContextDto};
use bookway_bbs::api::pb::{self as bbs_pb, bbs_client::BbsClient};
use bookway_commonlikestatus::api::pb::{
    self as like_pb, common_like_status_client::CommonLikeStatusClient,
};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum RecallClientError {
    #[error("recommend-recall grpc request failed: {0}")]
    Grpc(String),
}
#[derive(Debug, Error)]
pub(crate) enum BbsClientError {
    #[error("bbs request failed: {0}")]
    Request(String),
}
#[derive(Debug, Error)]
pub(crate) enum LikeStatusClientError {
    #[error("like status request failed: {0}")]
    Request(String),
}
#[derive(Debug, Error)]
pub(crate) enum ModelClientError {
    #[error("model request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("grpc request failed: {0}")]
    Grpc(String),
}

#[async_trait]
pub(crate) trait BbsContextDataSource: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, BbsClientError>;
}
pub(crate) struct GrpcBbsContextDataSource {
    client: BbsClient<tonic::transport::Channel>,
}
impl GrpcBbsContextDataSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: BbsClient::connect(base_url).await?,
        })
    }
}
#[async_trait]
impl BbsContextDataSource for GrpcBbsContextDataSource {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, BbsClientError> {
        let mut client = self.client.clone();
        let response = client
            .context(bbs_pb::ContextRequest {
                user_id: user_id.to_string(),
            })
            .await
            .map_err(|error| BbsClientError::Request(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| BbsClientError::Request(error.to_string()))
    }
}

#[async_trait]
pub(crate) trait LikeStatusDataSource: Send + Sync {
    async fn context(
        &self,
        user_id: &str,
        post_ids: Vec<String>,
    ) -> Result<ReactionContextDto, LikeStatusClientError>;
}
pub(crate) struct GrpcLikeStatusDataSource {
    client: CommonLikeStatusClient<tonic::transport::Channel>,
}
impl GrpcLikeStatusDataSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: CommonLikeStatusClient::connect(base_url).await?,
        })
    }
}
#[async_trait]
impl LikeStatusDataSource for GrpcLikeStatusDataSource {
    async fn context(
        &self,
        user_id: &str,
        post_ids: Vec<String>,
    ) -> Result<ReactionContextDto, LikeStatusClientError> {
        let mut client = self.client.clone();
        let response = client
            .context(like_pb::ContextRequest {
                request_json: serde_json::to_string(&bookway_api::ReactionContextRequest {
                    user_id: Some(user_id.to_string()),
                    post_ids: Some(post_ids.join(",")),
                })
                .map_err(|error| LikeStatusClientError::Request(error.to_string()))?,
            })
            .await
            .map_err(|error| LikeStatusClientError::Request(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| LikeStatusClientError::Request(error.to_string()))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Exposure {
    pub(crate) request_id: String,
    pub(crate) user_id: String,
    pub(crate) session_id: String,
    pub(crate) surface: String,
    pub(crate) post_ids: Vec<String>,
}
#[async_trait]
pub(crate) trait ExposureDataSource: Send + Sync {
    async fn record(&self, exposure: Exposure);
}
#[derive(Default)]
pub(crate) struct MemoryExposureDataSource {
    exposures: RwLock<Vec<Exposure>>,
}
#[async_trait]
impl ExposureDataSource for MemoryExposureDataSource {
    async fn record(&self, exposure: Exposure) {
        tracing::debug!(request_id=%exposure.request_id, selected=exposure.post_ids.len(), "recommendation exposure recorded");
        self.exposures.write().await.push(exposure);
    }
}
pub(crate) struct PostgresExposureDataSource {
    pool: sqlx::PgPool,
}
impl PostgresExposureDataSource {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl ExposureDataSource for PostgresExposureDataSource {
    async fn record(&self, exposure: Exposure) {
        let result = sqlx::query("INSERT INTO feed_exposures (request_id, user_id, session_id, surface, item_count) VALUES ($1, $2, $3, $4, $5)").bind(&exposure.request_id).bind(&exposure.user_id).bind(&exposure.session_id).bind(&exposure.surface).bind(i32::try_from(exposure.post_ids.len()).unwrap_or(i32::MAX)).execute(&self.pool).await;
        if let Err(error) = result {
            tracing::warn!(%error, "exposure persistence degraded");
        }
    }
}

pub(crate) type SharedBbsContextDataSource = Arc<dyn BbsContextDataSource>;
pub(crate) type SharedLikeStatusDataSource = Arc<dyn LikeStatusDataSource>;
pub(crate) type SharedExposureDataSource = Arc<dyn ExposureDataSource>;
