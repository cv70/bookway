use bookway_api::{CommentDto, CreateCommentRequest};

use crate::domain::{CommentError, Domain};

impl Domain {
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
    use std::sync::Arc;

    use super::*;
    use crate::{conf::Config, datasource::MemoryCommentRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().unwrap(),
                grpc_addr: "127.0.0.1:0".parse().unwrap(),
            },
            Arc::new(MemoryCommentRepository::default()),
        )
    }

    #[tokio::test]
    async fn rejects_empty_comments() {
        let result = domain()
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
        let result = domain()
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
