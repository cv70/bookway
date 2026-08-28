use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub(crate) struct Exposure {
    pub(crate) request_id: String,
    pub(crate) user_id: String,
    pub(crate) session_id: String,
    pub(crate) surface: String,
    pub(crate) pipeline_id: String,
    pub(crate) model_version: Option<String>,
    pub(crate) experiment_bucket: Option<String>,
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
    // The objective estimates the ranker produced for this serving. Recorded
    // so calibration and experiment evaluation work on what was predicted,
    // not only on the fused score.
    pub(crate) p_ctr: f64,
    pub(crate) p_cvr: f64,
    pub(crate) p_wegu: f64,
    // Serving-time feature values (JSON object) for offline model training.
    pub(crate) feature_snapshot: serde_json::Value,
    pub(crate) reasons: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExposureAttribution {
    pub(crate) request_id: String,
    pub(crate) session_id: String,
    pub(crate) content_id: String,
    pub(crate) position: u32,
}

#[derive(Debug, Error)]
pub(crate) enum ExposureError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("attribution position exceeds PostgreSQL integer range")]
    PositionOutOfRange,
}

#[async_trait]
pub(crate) trait ExposureDataSource: Send + Sync {
    async fn record(&self, exposure: Exposure) -> Result<(), ExposureError>;
    async fn recent_content_ids(
        &self,
        user_id: &str,
        surface: &str,
        limit: usize,
    ) -> HashSet<String>;
    async fn validate_attributions(
        &self,
        user_id: &str,
        attributions: &[ExposureAttribution],
    ) -> Result<Vec<bool>, ExposureError>;
}

pub(crate) type SharedExposureDataSource = Arc<dyn ExposureDataSource>;

#[cfg(test)]
mod tests {
    use super::{
        Exposure, ExposureAttribution, ExposureDataSource, ExposureItem, MemoryExposureDataSource,
    };

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
                model_version: Some("rank-v1".to_string()),
                experiment_bucket: Some("rank-v1-1".to_string()),
                candidate_count: 2,
                degraded: false,
                items: vec![ExposureItem {
                    position: 0,
                    content_id: "content-1".to_string(),
                    source: "recall:quality".to_string(),
                    score: 1.0,
                    p_ctr: 0.0,
                    p_cvr: 0.0,
                    p_wegu: 0.0,
                    feature_snapshot: serde_json::json!({}),
                    reasons: Vec::new(),
                }],
            })
            .await
            .expect("memory exposure record should succeed");

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

    #[tokio::test]
    async fn memory_attribution_validation_binds_user_session_content_and_position() {
        let source = MemoryExposureDataSource::default();
        source
            .record(Exposure {
                request_id: "request-1".to_string(),
                user_id: "user-1".to_string(),
                session_id: "session-1".to_string(),
                surface: "home".to_string(),
                pipeline_id: "pipeline".to_string(),
                model_version: None,
                experiment_bucket: None,
                candidate_count: 2,
                degraded: false,
                items: vec![ExposureItem {
                    position: 3,
                    content_id: "content-1".to_string(),
                    source: "recall:quality".to_string(),
                    score: 1.0,
                    p_ctr: 0.0,
                    p_cvr: 0.0,
                    p_wegu: 0.0,
                    feature_snapshot: serde_json::json!({}),
                    reasons: Vec::new(),
                }],
            })
            .await
            .expect("memory exposure record should succeed");

        let valid = source
            .validate_attributions(
                "user-1",
                &[
                    ExposureAttribution {
                        request_id: "request-1".to_string(),
                        session_id: "session-1".to_string(),
                        content_id: "content-1".to_string(),
                        position: 3,
                    },
                    ExposureAttribution {
                        request_id: "request-1".to_string(),
                        session_id: "session-2".to_string(),
                        content_id: "content-1".to_string(),
                        position: 3,
                    },
                    ExposureAttribution {
                        request_id: "request-1".to_string(),
                        session_id: "session-1".to_string(),
                        content_id: "content-1".to_string(),
                        position: 2,
                    },
                ],
            )
            .await
            .expect("memory validation should succeed");

        assert_eq!(valid, [true, false, false]);
    }
}

#[path = "memory_exposure_data_source.rs"]
mod memory_exposure_data_source;
pub(crate) use memory_exposure_data_source::MemoryExposureDataSource;
#[path = "postgres_exposure_data_source.rs"]
mod postgres_exposure_data_source;
pub(crate) use postgres_exposure_data_source::PostgresExposureDataSource;
