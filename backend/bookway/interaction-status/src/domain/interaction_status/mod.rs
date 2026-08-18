use crate::api::pb;

use crate::domain::{Domain, InteractionStatusError};

impl Domain {
    pub(crate) async fn context(
        &self,
        request: pb::ContextRequest,
    ) -> Result<pb::ReactionContext, InteractionStatusError> {
        let user_id = request.user_id.unwrap_or_else(|| "anonymous".to_string());
        let post_ids = request
            .post_ids
            .into_iter()
            .filter(|id| !id.trim().is_empty())
            .collect::<Vec<_>>();
        Ok(self.repository.context(&user_id, &post_ids).await?)
    }

    pub(crate) async fn set_reaction(
        &self,
        request: pb::SetReactionRequest,
    ) -> Result<pb::Reaction, InteractionStatusError> {
        let valid_reaction = pb::ReactionType::try_from(request.reaction).is_ok();
        if request.user_id.trim().is_empty() || request.post_id.trim().is_empty() || !valid_reaction
        {
            return Err(InteractionStatusError::Validation(
                "user_id, post_id and a valid reaction are required".to_string(),
            ));
        }
        Ok(self
            .repository
            .set_reaction(
                &request.user_id,
                &request.post_id,
                request.reaction,
                request.active,
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::api::pb;

    use super::*;
    use crate::{conf::Config, datasource::MemoryInteractionStatusRepository};

    #[tokio::test]
    async fn repeated_like_is_idempotent() {
        let domain = Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryInteractionStatusRepository::seeded()),
        );
        let request = pb::SetReactionRequest {
            user_id: "user-a".to_string(),
            post_id: "post-a".to_string(),
            reaction: pb::ReactionType::Like as i32,
            active: true,
        };
        let first = domain
            .set_reaction(request.clone())
            .await
            .expect("first reaction");
        let second = domain.set_reaction(request).await.expect("second reaction");
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
            Arc::new(MemoryInteractionStatusRepository::seeded()),
        );
        domain
            .set_reaction(pb::SetReactionRequest {
                user_id: "user-a".to_string(),
                post_id: "post-a".to_string(),
                reaction: pb::ReactionType::Hide as i32,
                active: true,
            })
            .await
            .expect("hide reaction");

        let context = domain
            .context(pb::ContextRequest {
                user_id: Some("user-a".to_string()),
                post_ids: vec!["post-a".to_string(), "post-b".to_string()],
            })
            .await
            .expect("reaction context");

        assert_eq!(context.hidden_post_ids, ["post-a"]);
    }
}
