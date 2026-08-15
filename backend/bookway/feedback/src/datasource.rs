use std::{collections::HashMap, sync::Arc};

use crate::api::pb;
use sqlx::FromRow;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("feedback {0} was not found")]
    NotFound(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored feedback is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
}

#[derive(Clone)]
pub(crate) struct FeedbackRepository {
    pool: Option<sqlx::PgPool>,
    memory: Arc<RwLock<MemoryFeedback>>,
}

#[derive(Default)]
struct MemoryFeedback {
    by_id: HashMap<String, pb::FeedbackItem>,
    idempotency: HashMap<(String, String), String>,
}

#[derive(FromRow)]
struct FeedbackRow {
    payload: serde_json::Value,
    status: String,
    resolution: Option<String>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl FeedbackRepository {
    pub(crate) fn new(pool: Option<sqlx::PgPool>) -> Self {
        Self {
            pool,
            memory: Arc::new(RwLock::new(MemoryFeedback::default())),
        }
    }

    pub(crate) async fn create(
        &self,
        feedback: pb::FeedbackItem,
        idempotency_key: Option<String>,
    ) -> Result<pb::FeedbackItem, RepositoryError> {
        let Some(pool) = &self.pool else {
            let Some(idempotency_key) = idempotency_key else {
                self.memory
                    .write()
                    .await
                    .by_id
                    .insert(feedback.id.clone(), feedback.clone());
                return Ok(feedback);
            };
            let mut memory = self.memory.write().await;
            let key = (feedback.user_id.clone(), idempotency_key);
            if let Some(existing_id) = memory.idempotency.get(&key)
                && let Some(existing) = memory.by_id.get(existing_id)
            {
                return Ok(existing.clone());
            }
            memory.idempotency.insert(key, feedback.id.clone());
            memory.by_id.insert(feedback.id.clone(), feedback.clone());
            return Ok(feedback);
        };
        let payload = serde_json::to_value(&feedback).map_err(RepositoryError::Serialization)?;
        let row = sqlx::query_as::<_, FeedbackRow>(
            "INSERT INTO user_feedback (id,user_id,category,content,contact,platform,app_version,status,idempotency_key,payload,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::timestamptz) ON CONFLICT (user_id,idempotency_key) DO UPDATE SET user_id=EXCLUDED.user_id RETURNING payload,status,resolution,created_at,updated_at",
        )
        .bind(&feedback.id)
        .bind(&feedback.user_id)
        .bind(category_name(feedback.category))
        .bind(&feedback.content)
        .bind(&feedback.contact)
        .bind(&feedback.platform)
        .bind(&feedback.app_version)
        .bind(status_name(feedback.status))
        .bind(idempotency_key)
        .bind(payload)
        .bind(&feedback.created_at)
        .fetch_one(pool)
        .await
        .map_err(RepositoryError::Database)?;
        hydrate(row)
    }

    pub(crate) async fn list(
        &self,
        user_id: Option<&str>,
        status: Option<i32>,
        limit: usize,
    ) -> Result<Vec<pb::FeedbackItem>, RepositoryError> {
        let Some(pool) = &self.pool else {
            let mut feedback = self
                .memory
                .read()
                .await
                .by_id
                .values()
                .filter(|feedback| user_id.is_none_or(|user_id| feedback.user_id == user_id))
                .filter(|feedback| status.is_none_or(|status| feedback.status == status))
                .cloned()
                .collect::<Vec<_>>();
            feedback.sort_by(|left, right| {
                right
                    .updated_at
                    .cmp(&left.updated_at)
                    .then_with(|| right.id.cmp(&left.id))
            });
            feedback.truncate(limit);
            return Ok(feedback);
        };
        let rows = sqlx::query_as::<_, FeedbackRow>(
            "SELECT payload,status,resolution,created_at,updated_at FROM user_feedback WHERE ($1::text IS NULL OR user_id=$1) AND ($2::text IS NULL OR status=$2) ORDER BY updated_at DESC,id DESC LIMIT $3",
        )
        .bind(user_id)
        .bind(status.map(status_name))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter().map(hydrate).collect()
    }

    pub(crate) async fn review(
        &self,
        feedback_id: &str,
        status: pb::FeedbackStatus,
        resolution: Option<String>,
    ) -> Result<pb::FeedbackItem, RepositoryError> {
        let Some(pool) = &self.pool else {
            let mut memory = self.memory.write().await;
            let feedback = memory
                .by_id
                .get_mut(feedback_id)
                .ok_or_else(|| RepositoryError::NotFound(feedback_id.to_string()))?;
            feedback.status = status as i32;
            feedback.resolution = resolution;
            feedback.updated_at = format_timestamp(OffsetDateTime::now_utc());
            return Ok(feedback.clone());
        };
        let mut transaction = pool.begin().await.map_err(RepositoryError::Database)?;
        let row = sqlx::query_as::<_, FeedbackRow>(
            "SELECT payload,status,resolution,created_at,updated_at FROM user_feedback WHERE id=$1 FOR UPDATE",
        )
        .bind(feedback_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(feedback_id.to_string()))?;
        let mut feedback = hydrate(row)?;
        feedback.status = status as i32;
        feedback.resolution = resolution;
        let payload = serde_json::to_value(&feedback).map_err(RepositoryError::Serialization)?;
        let row = sqlx::query_as::<_, FeedbackRow>(
            "UPDATE user_feedback SET status=$2,resolution=$3,payload=$4,updated_at=now() WHERE id=$1 RETURNING payload,status,resolution,created_at,updated_at",
        )
        .bind(feedback_id)
        .bind(status_name(status as i32))
        .bind(&feedback.resolution)
        .bind(payload)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        hydrate(row)
    }
}

fn hydrate(row: FeedbackRow) -> Result<pb::FeedbackItem, RepositoryError> {
    let mut feedback = serde_json::from_value::<pb::FeedbackItem>(row.payload)
        .map_err(RepositoryError::Serialization)?;
    feedback.status = parse_status(&row.status)?;
    feedback.resolution = row.resolution;
    feedback.created_at = format_timestamp(row.created_at);
    feedback.updated_at = format_timestamp(row.updated_at);
    Ok(feedback)
}

fn category_name(value: i32) -> &'static str {
    match pb::FeedbackCategory::try_from(value).unwrap_or(pb::FeedbackCategory::Other) {
        pb::FeedbackCategory::Bug => "bug",
        pb::FeedbackCategory::Feature => "feature",
        pb::FeedbackCategory::Experience => "experience",
        pb::FeedbackCategory::Content => "content",
        pb::FeedbackCategory::Other => "other",
    }
}

fn status_name(value: i32) -> &'static str {
    match pb::FeedbackStatus::try_from(value).unwrap_or(pb::FeedbackStatus::Pending) {
        pb::FeedbackStatus::Pending => "pending",
        pb::FeedbackStatus::Processing => "processing",
        pb::FeedbackStatus::Resolved => "resolved",
        pb::FeedbackStatus::Closed => "closed",
    }
}

fn parse_status(value: &str) -> Result<i32, RepositoryError> {
    let status = match value {
        "pending" => pb::FeedbackStatus::Pending,
        "processing" => pb::FeedbackStatus::Processing,
        "resolved" => pb::FeedbackStatus::Resolved,
        "closed" => pb::FeedbackStatus::Closed,
        value => {
            return Err(RepositoryError::Serialization(serde_json::Error::io(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown feedback status: {value}"),
                ),
            )));
        }
    };
    Ok(status as i32)
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
