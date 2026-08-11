use std::sync::Arc;

use bookway_api::{CommentDto, CreateCommentRequest};
use thiserror::Error;

use super::datasource::{CommentRepository, RepositoryError};

#[derive(Debug, Error)]
pub(crate) enum CommentError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct CommentService {
    repository: Arc<dyn CommentRepository>,
}

impl CommentService {
    pub(crate) fn new(repository: Arc<dyn CommentRepository>) -> Self {
        Self { repository }
    }

    pub(crate) async fn list(&self, post_id: &str) -> Result<Vec<CommentDto>, CommentError> {
        Ok(self.repository.list(post_id).await?)
    }

    pub(crate) async fn create(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
    ) -> Result<CommentDto, CommentError> {
        let body = request.body.trim();
        if body.is_empty() {
            return Err(CommentError::Validation("评论不能为空".to_string()));
        }
        if body.chars().count() > 1000 {
            return Err(CommentError::Validation(
                "评论不能超过 1000 个字符".to_string(),
            ));
        }
        Ok(self
            .repository
            .create(
                user_id,
                post_id,
                user_id,
                body.to_string(),
                request.parent_id,
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::datasource::MemoryCommentRepository;

    fn service() -> CommentService {
        CommentService::new(Arc::new(MemoryCommentRepository::default()))
    }

    #[tokio::test]
    async fn rejects_empty_comments() {
        let result = service()
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "   ".to_string(),
                    parent_id: None,
                },
            )
            .await;
        assert!(matches!(result, Err(CommentError::Validation(_))));
    }

    #[tokio::test]
    async fn validates_parent_on_the_same_post() {
        let result = service()
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "回复".to_string(),
                    parent_id: Some("missing".to_string()),
                },
            )
            .await;
        assert!(matches!(result, Err(CommentError::Repository(_))));
    }
}
