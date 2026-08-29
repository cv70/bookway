use crate::api::pb;

use crate::domain::{Domain, InteractionStatusError};

const MAX_IDENTIFIER_LENGTH: usize = 160;

impl Domain {
    pub(crate) async fn context(
        &self,
        request: pb::ContextRequest,
    ) -> Result<pb::ReactionContext, InteractionStatusError> {
        let user_id = request
            .user_id
            .map(|user_id| user_id.trim().to_string())
            .filter(|user_id| !user_id.is_empty());
        if user_id
            .as_ref()
            .is_some_and(|user_id| user_id.chars().count() > MAX_IDENTIFIER_LENGTH)
        {
            return Err(InteractionStatusError::Validation(
                "user id is too long".to_string(),
            ));
        }
        let mut post_ids = request
            .post_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .collect::<Vec<_>>();
        if post_ids
            .iter()
            .any(|id| id.chars().count() > MAX_IDENTIFIER_LENGTH)
        {
            return Err(InteractionStatusError::Validation(
                "post ids are too long".to_string(),
            ));
        }
        post_ids.retain(|id| !id.is_empty());
        if post_ids.len() > 500 {
            return Err(InteractionStatusError::Validation(
                "at most 500 post_ids may be requested".to_string(),
            ));
        }
        post_ids.sort_unstable();
        post_ids.dedup();
        let Some(user_id) = user_id else {
            // An optional identity means an anonymous viewer. Never map it to
            // a shared sentinel user, otherwise unrelated visitors can read
            // one another's cached reactions. Batch validation above still
            // applies so anonymous callers cannot bypass request bounds.
            return Ok(pb::ReactionContext::default());
        };
        Ok(self.dao.context(&user_id, &post_ids).await?)
    }

    pub(crate) async fn set_reaction(
        &self,
        mut request: pb::SetReactionRequest,
    ) -> Result<pb::Reaction, InteractionStatusError> {
        request.user_id = request.user_id.trim().to_string();
        request.post_id = request.post_id.trim().to_string();
        let valid_reaction = pb::ReactionType::try_from(request.reaction).is_ok();
        if request.user_id.is_empty()
            || request.post_id.is_empty()
            || request.user_id.chars().count() > MAX_IDENTIFIER_LENGTH
            || request.post_id.chars().count() > MAX_IDENTIFIER_LENGTH
            || !valid_reaction
        {
            return Err(InteractionStatusError::Validation(
                "user_id, post_id and a valid reaction are required".to_string(),
            ));
        }
        Ok(self
            .dao
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
    use crate::{conf::Config, datasource::MemoryInteractionStatusDao};

    #[tokio::test]
    async fn repeated_like_is_idempotent() {
        let domain = Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryInteractionStatusDao::seeded()),
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
        let domain = Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryInteractionStatusDao::seeded()),
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

    #[tokio::test]
    async fn context_rejects_oversized_batches() {
        let domain = Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryInteractionStatusDao::seeded()),
        );
        let error = domain
            .context(pb::ContextRequest {
                user_id: Some("user-a".to_string()),
                post_ids: (0..501).map(|index| format!("post-{index}")).collect(),
            })
            .await
            .expect_err("oversized context should be rejected");
        assert!(matches!(error, InteractionStatusError::Validation(_)));
    }

    #[tokio::test]
    async fn anonymous_context_does_not_read_the_seeded_user_bucket() {
        let domain = Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryInteractionStatusDao::seeded()),
        );
        let context = domain
            .context(pb::ContextRequest {
                user_id: None,
                post_ids: vec!["post-reading".to_string()],
            })
            .await
            .expect("anonymous context is a valid empty read");
        assert!(context.liked_post_ids.is_empty());
        assert!(context.bookmarked_post_ids.is_empty());
        assert!(context.hidden_post_ids.is_empty());
    }

    #[tokio::test]
    async fn anonymous_context_still_enforces_batch_bounds() {
        let domain = Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryInteractionStatusDao::seeded()),
        );
        let error = domain
            .context(pb::ContextRequest {
                user_id: None,
                post_ids: (0..501).map(|index| format!("post-{index}")).collect(),
            })
            .await
            .expect_err("anonymous callers must not bypass request bounds");
        assert!(matches!(error, InteractionStatusError::Validation(_)));
    }
}
