use crate::api::pb;

use crate::domain::{BbsError, Domain};

impl Domain {
    pub(crate) async fn context(
        &self,
        request: pb::ContextRequest,
    ) -> Result<pb::SocialContext, BbsError> {
        validate_user_id(&request.user_id)?;
        let user_id = request.user_id;
        Ok(self.dao.context(&user_id).await?)
    }

    pub(crate) async fn visibility_context(
        &self,
        request: pb::ContextRequest,
    ) -> Result<pb::SocialVisibility, BbsError> {
        validate_user_id(&request.user_id)?;
        Ok(self.dao.visibility_context(&request.user_id).await?)
    }

    pub(crate) async fn set_edge(
        &self,
        request: pb::SetEdgeRequest,
    ) -> Result<pb::SocialContext, BbsError> {
        validate_user_id(&request.user_id)?;
        validate_user_id(&request.target_user_id)?;
        if request.user_id == request.target_user_id {
            return Err(BbsError::Validation("不能对自己建立社交关系".to_string()));
        }
        Ok(self
            .dao
            .set_edge(
                &request.user_id,
                &request.target_user_id,
                pb::SocialEdgeType::try_from(request.edge)
                    .map_err(|_| BbsError::Validation("社交关系类型无效".to_string()))?,
                request.active,
            )
            .await?)
    }

    pub(crate) async fn list_route_participations(
        &self,
        request: pb::ContextRequest,
    ) -> Result<Vec<pb::RouteParticipation>, BbsError> {
        validate_user_id(&request.user_id)?;
        Ok(self.dao.list_route_participations(&request.user_id).await?)
    }

    pub(crate) async fn route_context(
        &self,
        request: pb::RouteContextRequest,
    ) -> Result<pb::RouteParticipationContext, BbsError> {
        validate_user_id(&request.user_id)?;
        if request.route_ids.len() > 500 {
            return Err(BbsError::Validation("单次最多查询 500 条路线".to_string()));
        }
        let mut route_ids = request
            .route_ids
            .into_iter()
            .map(|route_id| route_id.trim().to_string())
            .collect::<Vec<_>>();
        if route_ids.iter().any(|route_id| route_id.is_empty()) {
            return Err(BbsError::Validation("路线 ID 不能为空".to_string()));
        }
        route_ids.sort();
        route_ids.dedup();
        Ok(self.dao.route_context(&request.user_id, &route_ids).await?)
    }

    pub(crate) async fn set_route_participation(
        &self,
        request: pb::RouteParticipationRequest,
    ) -> Result<pb::RouteParticipationState, BbsError> {
        validate_user_id(&request.user_id)?;
        validate_identifier("路线 ID", &request.route_id)?;
        if let Some(journey_id) = request.private_journey_id.as_deref() {
            validate_identifier("私人路线 ID", journey_id)?;
        }
        if request
            .intent_version
            .is_some_and(|version| version > i64::MAX as u64)
        {
            return Err(BbsError::Validation("参与意图版本超出范围".to_string()));
        }
        Ok(self
            .dao
            .set_route_participation(
                &request.user_id,
                &request.route_id,
                request.active,
                request.private_journey_id,
                request.intent_version,
            )
            .await?)
    }
}

fn validate_user_id(user_id: &str) -> Result<(), BbsError> {
    validate_identifier("用户 ID", user_id)
}

fn validate_identifier(label: &str, value: &str) -> Result<(), BbsError> {
    if value.trim().is_empty() {
        return Err(BbsError::Validation(format!("{label} 不能为空")));
    }
    if value.chars().count() > 160 {
        return Err(BbsError::Validation(format!("{label} 不能超过 160 个字符")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::task::JoinSet;

    use super::*;
    use crate::{conf::Config, datasource::MemoryBbsDao};

    fn domain() -> Domain {
        Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryBbsDao::seeded()),
        )
    }

    #[tokio::test]
    async fn rejects_self_edges() {
        let result = domain()
            .set_edge(pb::SetEdgeRequest {
                user_id: "user-a".to_string(),
                target_user_id: "user-a".to_string(),
                edge: pb::SocialEdgeType::Follow as i32,
                active: true,
            })
            .await;
        assert!(matches!(result, Err(BbsError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_blank_relationship_users() {
        for (user_id, target_user_id) in [("", "user-b"), ("user-a", "   ")] {
            let result = domain()
                .set_edge(pb::SetEdgeRequest {
                    user_id: user_id.to_string(),
                    target_user_id: target_user_id.to_string(),
                    edge: pb::SocialEdgeType::Follow as i32,
                    active: true,
                })
                .await;
            assert!(matches!(result, Err(BbsError::Validation(_))));
        }
    }

    #[tokio::test]
    async fn block_removes_existing_follow() {
        let domain = domain();
        domain
            .set_edge(pb::SetEdgeRequest {
                user_id: "user-a".to_string(),
                target_user_id: "user-b".to_string(),
                edge: pb::SocialEdgeType::Follow as i32,
                active: true,
            })
            .await
            .expect("follow");
        let context = domain
            .set_edge(pb::SetEdgeRequest {
                user_id: "user-a".to_string(),
                target_user_id: "user-b".to_string(),
                edge: pb::SocialEdgeType::Block as i32,
                active: true,
            })
            .await
            .expect("block");
        assert!(context.followed_author_ids.is_empty());
        assert_eq!(context.blocked_author_ids, vec!["user-b"]);
    }

    #[tokio::test]
    async fn visibility_context_hides_outgoing_mutes_and_blocks_in_both_directions() {
        let domain = domain();
        for (source, target, edge) in [
            ("viewer", "author-blocked", pb::SocialEdgeType::Block),
            ("viewer", "author-muted", pb::SocialEdgeType::Mute),
            ("author-inbound", "viewer", pb::SocialEdgeType::Block),
        ] {
            domain
                .set_edge(pb::SetEdgeRequest {
                    user_id: source.to_string(),
                    target_user_id: target.to_string(),
                    edge: edge as i32,
                    active: true,
                })
                .await
                .expect("social edge");
        }

        let visibility = domain
            .visibility_context(pb::ContextRequest {
                user_id: "viewer".to_string(),
                post_ids: Vec::new(),
            })
            .await
            .expect("visibility context");

        assert_eq!(
            visibility.excluded_author_ids,
            vec!["author-blocked", "author-inbound", "author-muted"]
        );
    }

    #[tokio::test]
    async fn route_participation_is_idempotent_and_counted() {
        let domain = domain();
        let request = pb::RouteParticipationRequest {
            user_id: "user-a".to_string(),
            route_id: "route-a".to_string(),
            active: true,
            private_journey_id: Some("journey-a".to_string()),
            intent_version: None,
        };
        let first = domain
            .set_route_participation(request.clone())
            .await
            .expect("first join");
        let retry = domain
            .set_route_participation(request)
            .await
            .expect("idempotent retry");
        domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-b".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: None,
                intent_version: None,
            })
            .await
            .expect("second participant");

        assert_eq!(first.joined_at, retry.joined_at);
        assert_eq!(retry.participant_count, 1);
        let context = domain
            .route_context(pb::RouteContextRequest {
                user_id: "user-a".to_string(),
                route_ids: vec!["route-a".to_string()],
            })
            .await
            .expect("route context");
        assert_eq!(context.joined_route_ids, vec!["route-a"]);
        assert_eq!(context.participant_counts.get("route-a"), Some(&2));
    }

    #[tokio::test]
    async fn leaving_route_removes_it_from_context() {
        let domain = domain();
        domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: None,
                intent_version: None,
            })
            .await
            .expect("join");
        let state = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: false,
                private_journey_id: None,
                intent_version: None,
            })
            .await
            .expect("leave");

        assert!(!state.joined);
        assert_eq!(state.participant_count, 0);
        assert!(
            domain
                .list_route_participations(pb::ContextRequest {
                    user_id: "user-a".to_string(),
                    post_ids: Vec::new(),
                })
                .await
                .expect("participations")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn stale_intent_cannot_overwrite_a_newer_leave() {
        let domain = domain();
        let left = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: false,
                private_journey_id: None,
                intent_version: Some(2),
            })
            .await
            .expect("newer leave");
        let stale_join = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: Some("journey-a".to_string()),
                intent_version: Some(1),
            })
            .await
            .expect("stale join is ignored");

        assert!(!left.joined);
        assert!(!stale_join.joined);
        assert_eq!(stale_join.participant_count, 0);
    }

    #[tokio::test]
    async fn hot_route_accepts_many_concurrent_participants() {
        let domain = Arc::new(domain());
        let mut joins = JoinSet::new();
        for index in 0..256 {
            let domain = Arc::clone(&domain);
            joins.spawn(async move {
                domain
                    .set_route_participation(pb::RouteParticipationRequest {
                        user_id: format!("user-{index}"),
                        route_id: "route-hot".to_string(),
                        active: true,
                        private_journey_id: None,
                        intent_version: Some(1),
                    })
                    .await
            });
        }
        while let Some(result) = joins.join_next().await {
            result.expect("join task").expect("join hot route");
        }

        let context = domain
            .route_context(pb::RouteContextRequest {
                user_id: "observer".to_string(),
                route_ids: vec!["route-hot".to_string()],
            })
            .await
            .expect("hot route context");
        assert_eq!(context.participant_counts.get("route-hot"), Some(&256));
    }

    #[tokio::test]
    async fn concurrent_retries_for_one_user_only_count_once() {
        let domain = Arc::new(domain());
        let mut retries = JoinSet::new();
        for _ in 0..64 {
            let domain = Arc::clone(&domain);
            retries.spawn(async move {
                domain
                    .set_route_participation(pb::RouteParticipationRequest {
                        user_id: "user-a".to_string(),
                        route_id: "route-hot".to_string(),
                        active: true,
                        private_journey_id: Some("journey-a".to_string()),
                        intent_version: Some(1),
                    })
                    .await
            });
        }
        while let Some(result) = retries.join_next().await {
            result.expect("retry task").expect("retry join");
        }

        let state = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-hot".to_string(),
                active: true,
                private_journey_id: Some("journey-a".to_string()),
                intent_version: Some(1),
            })
            .await
            .expect("read state through retry");
        let context = domain
            .route_context(pb::RouteContextRequest {
                user_id: "user-a".to_string(),
                route_ids: vec!["route-hot".to_string()],
            })
            .await
            .expect("route context");
        assert_eq!(state.participant_count, 1);
        assert_eq!(context.participant_counts.get("route-hot"), Some(&1));
    }

    #[tokio::test]
    async fn a_newer_rejoin_increments_the_count_once() {
        let domain = domain();
        domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: false,
                private_journey_id: None,
                intent_version: Some(2),
            })
            .await
            .expect("leave route");
        let stale = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: Some("journey-stale".to_string()),
                intent_version: Some(1),
            })
            .await
            .expect("ignore stale join");
        let rejoined = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: Some("journey-current".to_string()),
                intent_version: Some(3),
            })
            .await
            .expect("rejoin route");
        let retry = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "user-a".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: Some("journey-current".to_string()),
                intent_version: Some(3),
            })
            .await
            .expect("retry rejoin");

        assert!(!stale.joined);
        assert!(rejoined.joined);
        assert_eq!(rejoined.participant_count, 1);
        assert_eq!(retry.participant_count, 1);
    }
}
