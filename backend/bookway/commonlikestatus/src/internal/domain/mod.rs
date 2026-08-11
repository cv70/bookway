use std::sync::Arc;

use bookway_api::{ReactionContextDto, ReactionContextRequest, ReactionDto, ReactionRequest};

use super::datasource::LikeStatusRepository;
use super::datasource::RepositoryError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum LikeStatusError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct LikeStatusService {
    repository: Arc<dyn LikeStatusRepository>,
}

impl LikeStatusService {
    pub(crate) fn new(repository: Arc<dyn LikeStatusRepository>) -> Self {
        Self { repository }
    }

    pub(crate) async fn context(
        &self,
        request: ReactionContextRequest,
    ) -> Result<ReactionContextDto, LikeStatusError> {
        let user_id = request.user_id.unwrap_or_else(|| "anonymous".to_string());
        let post_ids = request
            .post_ids
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        Ok(self.repository.context(&user_id, &post_ids).await?)
    }

    pub(crate) async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, LikeStatusError> {
        Ok(self
            .repository
            .set_reaction(user_id, post_id, request.reaction, request.active)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use bookway_api::{ReactionRequest, ReactionTypeDto};

    use super::*;
    use crate::internal::datasource::MemoryLikeStatusRepository;

    #[tokio::test]
    async fn repeated_like_is_idempotent() {
        let service = LikeStatusService::new(Arc::new(MemoryLikeStatusRepository::seeded()));
        let request = ReactionRequest {
            reaction: ReactionTypeDto::Like,
            active: true,
        };
        let first = service
            .set_reaction("user-a", "post-a", request.clone())
            .await
            .expect("first reaction");
        let second = service
            .set_reaction("user-a", "post-a", request)
            .await
            .expect("second reaction");
        assert_eq!(first.count, second.count);
        assert_eq!(second.count, 1);
    }
}
