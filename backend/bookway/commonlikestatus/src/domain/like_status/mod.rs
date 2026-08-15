use bookway_api::{ReactionContextDto, ReactionContextRequest, ReactionDto, ReactionRequest};

use crate::domain::{Domain, LikeStatusError};

impl Domain {
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
    use std::sync::Arc;

    use bookway_api::{ReactionContextRequest, ReactionRequest, ReactionTypeDto};

    use super::*;
    use crate::{conf::Config, datasource::MemoryLikeStatusRepository};

    #[tokio::test]
    async fn repeated_like_is_idempotent() {
        let domain = Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryLikeStatusRepository::seeded()),
        );
        let request = ReactionRequest {
            reaction: ReactionTypeDto::Like,
            active: true,
        };
        let first = domain
            .set_reaction("user-a", "post-a", request.clone())
            .await
            .expect("first reaction");
        let second = domain
            .set_reaction("user-a", "post-a", request)
            .await
            .expect("second reaction");
        assert_eq!(first.count, second.count);
        assert_eq!(second.count, 1);
    }

    #[tokio::test]
    async fn hide_is_returned_in_the_users_recommendation_context() {
        let domain = Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryLikeStatusRepository::seeded()),
        );
        domain
            .set_reaction(
                "user-a",
                "post-a",
                ReactionRequest {
                    reaction: ReactionTypeDto::Hide,
                    active: true,
                },
            )
            .await
            .expect("hide reaction");

        let context = domain
            .context(ReactionContextRequest {
                user_id: Some("user-a".to_string()),
                post_ids: Some("post-a,post-b".to_string()),
            })
            .await
            .expect("reaction context");

        assert_eq!(context.hidden_post_ids, ["post-a"]);
    }
}
