use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use super::api::CommentDto;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("parent comment {0} was not found on this post")]
    ParentNotFound(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[async_trait]
pub(crate) trait CommentRepository: Send + Sync {
    async fn list(&self, post_id: &str) -> Result<Vec<CommentDto>, RepositoryError>;
    async fn create(
        &self,
        user_id: &str,
        post_id: &str,
        author_name: &str,
        body: String,
        parent_id: Option<String>,
    ) -> Result<CommentDto, RepositoryError>;
}

#[derive(Default)]
pub(crate) struct MemoryCommentRepository {
    comments: RwLock<HashMap<String, Vec<CommentDto>>>,
}

#[async_trait]
impl CommentRepository for MemoryCommentRepository {
    async fn list(&self, post_id: &str) -> Result<Vec<CommentDto>, RepositoryError> {
        Ok(self
            .comments
            .read()
            .await
            .get(post_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn create(
        &self,
        user_id: &str,
        post_id: &str,
        author_name: &str,
        body: String,
        parent_id: Option<String>,
    ) -> Result<CommentDto, RepositoryError> {
        let mut comments = self.comments.write().await;
        if let Some(parent_id) = parent_id.as_deref()
            && !comments
                .get(post_id)
                .is_some_and(|items| items.iter().any(|comment| comment.id == parent_id))
        {
            return Err(RepositoryError::ParentNotFound(parent_id.to_string()));
        }
        let comment = CommentDto {
            id: uuid::Uuid::now_v7().to_string(),
            post_id: post_id.to_string(),
            author_id: user_id.to_string(),
            author_name: author_name.to_string(),
            body,
            parent_id,
            like_count: 0,
            created_at: now_ms(),
            status: bookway_api::ContentStatusDto::Published,
        };
        comments
            .entry(post_id.to_string())
            .or_default()
            .push(comment.clone());
        Ok(comment)
    }
}

pub(crate) struct PostgresCommentRepository {
    pool: sqlx::PgPool,
}

impl PostgresCommentRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommentRepository for PostgresCommentRepository {
    async fn list(&self, post_id: &str) -> Result<Vec<CommentDto>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, String, i64, time::OffsetDateTime)>(
            "SELECT id, author_id, parent_id, body, like_count, created_at FROM comments WHERE post_id = $1 AND deleted_at IS NULL ORDER BY created_at ASC",
        )
        .bind(post_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, author_id, parent_id, body, like_count, created_at)| CommentDto {
                    id,
                    post_id: post_id.to_string(),
                    author_id: author_id.clone(),
                    author_name: author_id,
                    body,
                    parent_id,
                    like_count: like_count.max(0) as u64,
                    created_at: created_at
                        .unix_timestamp_nanos()
                        .div_euclid(1_000_000)
                        .to_string(),
                    status: bookway_api::ContentStatusDto::Published,
                },
            )
            .collect())
    }

    async fn create(
        &self,
        user_id: &str,
        post_id: &str,
        author_name: &str,
        body: String,
        parent_id: Option<String>,
    ) -> Result<CommentDto, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(parent) = parent_id.as_deref() {
            let same_post = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND post_id = $2 AND deleted_at IS NULL)",
            )
            .bind(parent).bind(post_id).fetch_one(&mut *tx).await
            .map_err(RepositoryError::Database)?;
            if !same_post {
                return Err(RepositoryError::ParentNotFound(parent.to_string()));
            }
        }
        let id = uuid::Uuid::now_v7().to_string();
        let (created_at,) = sqlx::query_as::<_, (time::OffsetDateTime,)>(
            "INSERT INTO comments (id, post_id, author_id, parent_id, body, moderation_state) VALUES ($1,$2,$3,$4,$5,'published') RETURNING created_at",
        )
        .bind(&id).bind(post_id).bind(user_id).bind(&parent_id).bind(&body)
        .fetch_one(&mut *tx).await.map_err(RepositoryError::Database)?;
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(CommentDto {
            id,
            post_id: post_id.to_string(),
            author_id: user_id.to_string(),
            author_name: author_name.to_string(),
            body,
            parent_id,
            like_count: 0,
            created_at: created_at
                .unix_timestamp_nanos()
                .div_euclid(1_000_000)
                .to_string(),
            status: bookway_api::ContentStatusDto::Published,
        })
    }
}

fn now_ms() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
