use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use bookway_api::{
    ReactionContextDto, RouteParticipationContextDto, SocialContextDto, SocialVisibilityDto,
};
use bookway_bbs::api::pb::{self as bbs_pb, bbs_client::BbsClient};
use bookway_commonlikestatus::api::pb::{
    self as like_pb, common_like_status_client::CommonLikeStatusClient,
};
use thiserror::Error;
use tokio::sync::RwLock;
use tonic::Request;

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
    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<SocialVisibilityDto, BbsClientError>;
    async fn route_context(
        &self,
        user_id: &str,
        route_ids: Vec<String>,
    ) -> Result<RouteParticipationContextDto, BbsClientError>;
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
            .context(privileged_bbs_request(bbs_pb::ContextRequest {
                user_id: user_id.to_string(),
            })?)
            .await
            .map_err(|error| BbsClientError::Request(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| BbsClientError::Request(error.to_string()))
    }

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<SocialVisibilityDto, BbsClientError> {
        let mut client = self.client.clone();
        let response = client
            .visibility_context(privileged_bbs_request(bbs_pb::ContextRequest {
                user_id: user_id.to_string(),
            })?)
            .await
            .map_err(|error| BbsClientError::Request(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| BbsClientError::Request(error.to_string()))
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: Vec<String>,
    ) -> Result<RouteParticipationContextDto, BbsClientError> {
        let mut client = self.client.clone();
        let response = client
            .route_context(privileged_bbs_request(bbs_pb::RouteContextRequest {
                user_id: user_id.to_string(),
                route_ids,
            })?)
            .await
            .map_err(|error| BbsClientError::Request(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| BbsClientError::Request(error.to_string()))
    }
}

fn privileged_bbs_request<T>(message: T) -> Result<Request<T>, BbsClientError> {
    bookway_runtime::grpc_service_request(message)
        .map_err(|error| BbsClientError::Request(error.to_string()))
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
    pub(crate) pipeline_id: String,
    pub(crate) candidate_count: usize,
    pub(crate) degraded: bool,
    pub(crate) items: Vec<ExposureItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExposureItem {
    pub(crate) position: usize,
    pub(crate) content_id: String,
    pub(crate) source: String,
    pub(crate) score: f64,
    pub(crate) reasons: Vec<String>,
}
#[async_trait]
pub(crate) trait ExposureDataSource: Send + Sync {
    async fn record(&self, exposure: Exposure);
    async fn recent_content_ids(
        &self,
        user_id: &str,
        surface: &str,
        limit: usize,
    ) -> HashSet<String>;
}
#[derive(Default)]
pub(crate) struct MemoryExposureDataSource {
    exposures: RwLock<Vec<Exposure>>,
}
#[async_trait]
impl ExposureDataSource for MemoryExposureDataSource {
    async fn record(&self, exposure: Exposure) {
        tracing::debug!(request_id=%exposure.request_id, selected=exposure.items.len(), "recommendation exposure recorded");
        let mut exposures = self.exposures.write().await;
        exposures.push(exposure);
        const MAX_EXPOSURES: usize = 10_000;
        if exposures.len() > MAX_EXPOSURES {
            let overflow = exposures.len() - MAX_EXPOSURES;
            exposures.drain(..overflow);
        }
    }

    async fn recent_content_ids(
        &self,
        user_id: &str,
        surface: &str,
        limit: usize,
    ) -> HashSet<String> {
        let exposures = self.exposures.read().await;
        let mut content_ids = HashSet::new();
        for exposure in exposures
            .iter()
            .rev()
            .filter(|exposure| exposure.user_id == user_id && exposure.surface == surface)
        {
            for item in &exposure.items {
                content_ids.insert(item.content_id.clone());
                if content_ids.len() >= limit {
                    return content_ids;
                }
            }
        }
        content_ids
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
        let mut transaction = match self.pool.begin().await {
            Ok(transaction) => transaction,
            Err(error) => {
                tracing::warn!(%error, "exposure persistence degraded");
                return;
            }
        };
        let selected_count = i32::try_from(exposure.items.len()).unwrap_or(i32::MAX);
        let candidate_count = i32::try_from(exposure.candidate_count).unwrap_or(i32::MAX);
        let header = sqlx::query(
            "INSERT INTO feed_exposures (request_id, user_id, session_id, surface, pipeline_id, candidate_count, selected_count, degraded) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&exposure.request_id)
        .bind(&exposure.user_id)
        .bind(&exposure.session_id)
        .bind(&exposure.surface)
        .bind(&exposure.pipeline_id)
        .bind(candidate_count)
        .bind(selected_count)
        .bind(exposure.degraded)
        .execute(&mut *transaction)
        .await;
        if let Err(error) = header {
            tracing::warn!(%error, "exposure persistence degraded");
            return;
        }
        for item in &exposure.items {
            let position = i32::try_from(item.position).unwrap_or(i32::MAX);
            let result = sqlx::query(
                "INSERT INTO feed_exposure_items (request_id, position, content_id, source, score, reasons) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(&exposure.request_id)
            .bind(position)
            .bind(&item.content_id)
            .bind(&item.source)
            .bind(item.score)
            .bind(serde_json::json!(item.reasons))
            .execute(&mut *transaction)
            .await;
            if let Err(error) = result {
                tracing::warn!(%error, "exposure item persistence degraded");
                return;
            }
        }
        if let Err(error) = transaction.commit().await {
            tracing::warn!(%error, "exposure persistence degraded");
        }
    }

    async fn recent_content_ids(
        &self,
        user_id: &str,
        surface: &str,
        limit: usize,
    ) -> HashSet<String> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT item.content_id FROM feed_exposure_items AS item INNER JOIN feed_exposures AS exposure ON exposure.request_id = item.request_id WHERE exposure.user_id = $1 AND exposure.surface = $2 AND exposure.created_at > now() - interval '7 days' ORDER BY exposure.created_at DESC, item.position ASC LIMIT $3",
        )
        .bind(user_id)
        .bind(surface)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(rows) => rows.into_iter().collect(),
            Err(error) => {
                tracing::warn!(%error, "served history read degraded");
                HashSet::new()
            }
        }
    }
}

pub(crate) type SharedBbsContextDataSource = Arc<dyn BbsContextDataSource>;
pub(crate) type SharedLikeStatusDataSource = Arc<dyn LikeStatusDataSource>;
pub(crate) type SharedExposureDataSource = Arc<dyn ExposureDataSource>;

#[cfg(test)]
mod tests {
    use super::{Exposure, ExposureDataSource, ExposureItem, MemoryExposureDataSource};

    #[tokio::test]
    async fn memory_history_returns_recently_served_content_for_the_same_user() {
        let source = MemoryExposureDataSource::default();
        source
            .record(Exposure {
                request_id: "request-1".to_string(),
                user_id: "user-1".to_string(),
                session_id: "session-1".to_string(),
                surface: "home".to_string(),
                pipeline_id: "pipeline".to_string(),
                candidate_count: 2,
                degraded: false,
                items: vec![ExposureItem {
                    position: 0,
                    content_id: "content-1".to_string(),
                    source: "recall:quality".to_string(),
                    score: 1.0,
                    reasons: Vec::new(),
                }],
            })
            .await;

        let history = source.recent_content_ids("user-1", "home", 20).await;

        assert!(history.contains("content-1"));
        assert!(
            source
                .recent_content_ids("user-1", "following", 20)
                .await
                .is_empty()
        );
        assert!(
            source
                .recent_content_ids("user-2", "home", 20)
                .await
                .is_empty()
        );
    }
}
