use bookway_api::{FollowRequest, SocialContextDto, SocialContextRequest};

use crate::domain::{BbsError, Domain};

impl Domain {
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
    use std::sync::Arc;

    use bookway_api::SocialEdgeTypeDto;

    use super::*;
    use crate::{conf::Config, datasource::MemoryBbsRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().unwrap(),
                grpc_addr: "127.0.0.1:0".parse().unwrap(),
            },
            Arc::new(MemoryBbsRepository::seeded()),
        )
    }

    #[tokio::test]
    async fn rejects_self_edges() {
        let result = domain()
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
        let domain = domain();
        domain
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
        let context = domain
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
