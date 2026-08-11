use std::sync::Arc;

use async_trait::async_trait;
use bookway_api::{
    ApiResponse, ContentPageDto, ContentQueryRequest, ReactionContextDto, ReactionContextRequest,
    SocialContextDto, SocialContextRequest,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum BbsLinkClientError {
    #[error("bbs-link request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[async_trait]
pub(crate) trait BbsLinkDataSource: Send + Sync {
    async fn list(
        &self,
        strategy: &str,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<ContentPageDto, BbsLinkClientError>;
}

pub(crate) struct HttpBbsLinkDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpBbsLinkDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl BbsLinkDataSource for HttpBbsLinkDataSource {
    async fn list(
        &self,
        strategy: &str,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<ContentPageDto, BbsLinkClientError> {
        Ok(self
            .client
            .get(format!("{}/internal/v1/contents", self.base_url))
            .query(&ContentQueryRequest {
                cursor,
                limit: Some(limit),
                status: Some(bookway_api::ContentStatusDto::Published),
                strategy: Some(strategy.to_string()),
                ids: None,
            })
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<ContentPageDto>>()
            .await?
            .data)
    }
}

#[derive(Debug, Error)]
pub(crate) enum BbsClientError {
    #[error("bbs request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[async_trait]
pub(crate) trait BbsContextDataSource: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, BbsClientError>;
}

pub(crate) struct HttpBbsContextDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpBbsContextDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl BbsContextDataSource for HttpBbsContextDataSource {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, BbsClientError> {
        Ok(self
            .client
            .get(format!("{}/internal/v1/social-context", self.base_url))
            .query(&SocialContextRequest {
                user_id: Some(user_id.to_string()),
                post_ids: None,
            })
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<SocialContextDto>>()
            .await?
            .data)
    }
}

#[derive(Debug, Error)]
pub(crate) enum LikeStatusClientError {
    #[error("commonlikestatus request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[async_trait]
pub(crate) trait LikeStatusDataSource: Send + Sync {
    async fn context(
        &self,
        user_id: &str,
        post_ids: Vec<String>,
    ) -> Result<ReactionContextDto, LikeStatusClientError>;
}

pub(crate) struct HttpLikeStatusDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpLikeStatusDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl LikeStatusDataSource for HttpLikeStatusDataSource {
    async fn context(
        &self,
        user_id: &str,
        post_ids: Vec<String>,
    ) -> Result<ReactionContextDto, LikeStatusClientError> {
        Ok(self
            .client
            .get(format!("{}/internal/v1/reaction-context", self.base_url))
            .query(&ReactionContextRequest {
                user_id: Some(user_id.to_string()),
                post_ids: Some(post_ids.join(",")),
            })
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<ReactionContextDto>>()
            .await?
            .data)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Exposure {
    pub(crate) request_id: String,
    pub(crate) user_id: String,
    pub(crate) session_id: String,
    pub(crate) surface: String,
    pub(crate) post_ids: Vec<String>,
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
        let result: Result<(), sqlx::Error> = async {
            let mut tx = self.pool.begin().await?;
            let inserted = sqlx::query("INSERT INTO feed_exposures (request_id,user_id,session_id,surface,pipeline_id,candidate_count,selected_count) VALUES ($1,$2,$3,$4,'bookway-recommend-main-v1',$5,$5) ON CONFLICT (request_id) DO NOTHING")
                .bind(&exposure.request_id).bind(&exposure.user_id).bind(&exposure.session_id).bind(&exposure.surface).bind(i32::try_from(exposure.post_ids.len()).unwrap_or(i32::MAX)).execute(&mut *tx).await?;
            if inserted.rows_affected() > 0 {
                for (position, content_id) in exposure.post_ids.iter().enumerate() {
                    sqlx::query("INSERT INTO feed_exposure_items (request_id,position,content_id,source,score,reasons) VALUES ($1,$2,$3,'recommend-main',0,'[]')")
                        .bind(&exposure.request_id).bind(i32::try_from(position).unwrap_or(i32::MAX)).bind(content_id).execute(&mut *tx).await?;
                }
            }
            tx.commit().await?;
            Ok(())
        }.await;
        if let Err(error) = result {
            tracing::warn!(%error, request_id=%exposure.request_id, "exposure persistence degraded");
        }
    }
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
        tracing::debug!(
            request_id = %exposure.request_id,
            selected = exposure.post_ids.len(),
            "recommendation exposure recorded"
        );
        self.exposures.write().await.push(exposure);
    }
}

pub(crate) type SharedBbsLinkDataSource = Arc<dyn BbsLinkDataSource>;
pub(crate) type SharedBbsContextDataSource = Arc<dyn BbsContextDataSource>;
pub(crate) type SharedLikeStatusDataSource = Arc<dyn LikeStatusDataSource>;
pub(crate) type SharedExposureDataSource = Arc<dyn ExposureDataSource>;

#[derive(Debug, Error)]
pub(crate) enum ModelClientError {
    #[error("feature or rank service request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Serialize)]
struct FeatureRequest<'a> {
    user_id: &'a str,
    content_ids: Vec<String>,
}
#[derive(Deserialize)]
struct FeatureResponse {
    features: serde_json::Value,
}
#[derive(Serialize)]
struct RankRequest<'a> {
    user_id: &'a str,
    candidates: Vec<RankCandidate>,
    features: serde_json::Value,
}
#[derive(Serialize)]
struct RankCandidate {
    content_id: String,
    recall_score: f64,
    quality_score: f64,
    freshness: f64,
}
#[derive(Deserialize)]
pub(crate) struct RankedItem {
    pub(crate) content_id: String,
    pub(crate) score: f64,
}

#[async_trait]
pub(crate) trait ModelDataSource: Send + Sync {
    async fn rank(
        &self,
        user_id: &str,
        candidates: Vec<(String, f64, f64, f64)>,
    ) -> Result<Vec<RankedItem>, ModelClientError>;
}

pub(crate) struct HttpModelDataSource {
    client: reqwest::Client,
    feature_url: String,
    rank_url: String,
}
impl HttpModelDataSource {
    pub(crate) fn new(feature_url: String, rank_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            feature_url: feature_url.trim_end_matches('/').to_string(),
            rank_url: rank_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl ModelDataSource for HttpModelDataSource {
    async fn rank(
        &self,
        user_id: &str,
        candidates: Vec<(String, f64, f64, f64)>,
    ) -> Result<Vec<RankedItem>, ModelClientError> {
        let ids = candidates
            .iter()
            .map(|candidate| candidate.0.clone())
            .collect::<Vec<_>>();
        let features = self
            .client
            .post(format!("{}/internal/v1/features", self.feature_url))
            .json(&FeatureRequest {
                user_id,
                content_ids: ids,
            })
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<FeatureResponse>>()
            .await?
            .data
            .features;
        let candidates = candidates
            .into_iter()
            .map(
                |(content_id, recall_score, quality_score, freshness)| RankCandidate {
                    content_id,
                    recall_score,
                    quality_score,
                    freshness,
                },
            )
            .collect();
        Ok(self
            .client
            .post(format!("{}/internal/v1/rank", self.rank_url))
            .json(&RankRequest {
                user_id,
                candidates,
                features,
            })
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<Vec<RankedItem>>>()
            .await?
            .data)
    }
}

pub(crate) type SharedModelDataSource = Arc<dyn ModelDataSource>;
