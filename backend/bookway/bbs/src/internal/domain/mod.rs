use std::sync::Arc;

use bookway_api::{FollowRequest, SocialContextDto, SocialContextRequest};
use thiserror::Error;

use super::datasource::{BbsRepository, RepositoryError};

#[derive(Debug, Error)]
pub(crate) enum BbsError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct BbsService {
    repository: Arc<dyn BbsRepository>,
}

impl BbsService {
    pub(crate) fn new(repository: Arc<dyn BbsRepository>) -> Self {
        Self { repository }
    }

    pub(crate) async fn context(
        &self,
        request: SocialContextRequest,
    ) -> Result<SocialContextDto, BbsError> {
        let user_id = request.user_id.unwrap_or_else(|| "anonymous".to_string());
        Ok(self.repository.context(&user_id).await?)
    }

    pub(crate) async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<SocialContextDto, BbsError> {
        if user_id == target_user_id {
            return Err(BbsError::Validation("不能对自己建立社交关系".to_string()));
        }
        Ok(self
            .repository
            .set_edge(user_id, target_user_id, request.edge, request.active)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use bookway_api::SocialEdgeTypeDto;

    use super::*;
    use crate::internal::datasource::MemoryBbsRepository;

    fn service() -> BbsService {
        BbsService::new(Arc::new(MemoryBbsRepository::seeded()))
    }

    #[tokio::test]
    async fn rejects_self_edges() {
        let result = service()
            .set_edge(
                "user-a",
                "user-a",
                FollowRequest {
                    edge: SocialEdgeTypeDto::Follow,
                    active: true,
                },
            )
            .await;
        assert!(matches!(result, Err(BbsError::Validation(_))));
    }

    #[tokio::test]
    async fn block_removes_existing_follow() {
        let service = service();
        service
            .set_edge(
                "user-a",
                "user-b",
                FollowRequest {
                    edge: SocialEdgeTypeDto::Follow,
                    active: true,
                },
            )
            .await
            .expect("follow");
        let context = service
            .set_edge(
                "user-a",
                "user-b",
                FollowRequest {
                    edge: SocialEdgeTypeDto::Block,
                    active: true,
                },
            )
            .await
            .expect("block");
        assert!(context.followed_author_ids.is_empty());
        assert_eq!(context.blocked_author_ids, vec!["user-b"]);
    }
}
