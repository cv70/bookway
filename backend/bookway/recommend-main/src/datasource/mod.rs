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
#[derive(Default)]
pub(crate) struct MemoryExposureDataSource {
    exposures: RwLock<Vec<Exposure>>,
}
#[async_trait]
impl ExposureDataSource for MemoryExposureDataSource {
    async fn record(&self, exposure: Exposure) -> Result<(), ExposureError> {
        tracing::debug!(request_id=%exposure.request_id, selected=exposure.items.len(), "recommendation exposure recorded");
        let mut exposures = self.exposures.write().await;
        exposures.push(exposure);
        const MAX_EXPOSURES: usize = 10_000;
        if exposures.len() > MAX_EXPOSURES {
            let overflow = exposures.len() - MAX_EXPOSURES;
            exposures.drain(..overflow);
        }
        Ok(())
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

    async fn validate_attributions(
        &self,
        user_id: &str,
        attributions: &[ExposureAttribution],
    ) -> Result<Vec<bool>, ExposureError> {
        let exposures = self.exposures.read().await;
        Ok(attributions
            .iter()
            .map(|attribution| {
                exposures.iter().any(|exposure| {
                    exposure.request_id == attribution.request_id
                        && exposure.user_id == user_id
                        && exposure.session_id == attribution.session_id
                        && exposure.items.iter().any(|item| {
                            usize::try_from(attribution.position)
                                .is_ok_and(|position| position == item.position)
                                && item.content_id == attribution.content_id
                        })
                })
            })
            .collect())
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
    async fn record(&self, exposure: Exposure) -> Result<(), ExposureError> {
        let mut transaction = self.pool.begin().await?;
        let selected_count = i32::try_from(exposure.items.len()).unwrap_or(i32::MAX);
        let candidate_count = i32::try_from(exposure.candidate_count).unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO feed_exposures (request_id, user_id, session_id, surface, pipeline_id, model_version, experiment_bucket, candidate_count, selected_count, degraded) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(&exposure.request_id)
        .bind(&exposure.user_id)
        .bind(&exposure.session_id)
        .bind(&exposure.surface)
        .bind(&exposure.pipeline_id)
        .bind(&exposure.model_version)
        .bind(&exposure.experiment_bucket)
        .bind(candidate_count)
        .bind(selected_count)
        .bind(exposure.degraded)
        .execute(&mut *transaction)
        .await?;
        for item in &exposure.items {
            let position = i32::try_from(item.position).unwrap_or(i32::MAX);
            sqlx::query(
                "INSERT INTO feed_exposure_items (request_id, position, content_id, source, score, reasons) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(&exposure.request_id)
            .bind(position)
            .bind(&item.content_id)
            .bind(&item.source)
            .bind(item.score)
            .bind(serde_json::json!(item.reasons))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
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

    async fn validate_attributions(
        &self,
        user_id: &str,
        attributions: &[ExposureAttribution],
    ) -> Result<Vec<bool>, ExposureError> {
        if attributions.is_empty() {
            return Ok(Vec::new());
        }
        let positions = attributions
            .iter()
            .map(|attribution| i32::try_from(attribution.position))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ExposureError::PositionOutOfRange)?;
        let request_ids = attributions
            .iter()
            .map(|attribution| attribution.request_id.clone())
            .collect::<Vec<_>>();
        let session_ids = attributions
            .iter()
            .map(|attribution| attribution.session_id.clone())
            .collect::<Vec<_>>();
        let content_ids = attributions
            .iter()
            .map(|attribution| attribution.content_id.clone())
            .collect::<Vec<_>>();
        let rows = sqlx::query_as::<_, (i64, bool)>(
            "SELECT input.ordinality, EXISTS (SELECT 1 FROM feed_exposures AS exposure INNER JOIN feed_exposure_items AS item ON item.request_id = exposure.request_id WHERE exposure.request_id = input.request_id AND exposure.user_id = $1 AND exposure.session_id = input.session_id AND item.position = input.position AND item.content_id = input.content_id) AS valid FROM unnest($2::text[], $3::text[], $4::text[], $5::integer[]) WITH ORDINALITY AS input(request_id, session_id, content_id, position, ordinality) ORDER BY input.ordinality",
        )
        .bind(user_id)
        .bind(request_ids)
        .bind(session_ids)
        .bind(content_ids)
        .bind(positions)
        .fetch_all(&self.pool)
        .await?;
        let mut valid = vec![false; attributions.len()];
        for (ordinality, is_valid) in rows {
            let index = usize::try_from(ordinality)
                .ok()
                .and_then(|value| value.checked_sub(1));
            if let Some(index) = index.filter(|index| *index < valid.len()) {
                valid[index] = is_valid;
            }
        }
        Ok(valid)
    }
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
