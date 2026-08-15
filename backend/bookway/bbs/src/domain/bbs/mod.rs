use bookway_api::{
    FollowRequest, RouteParticipationContextDto, RouteParticipationDto, RouteParticipationStateDto,
    SetRouteParticipationRequest, SocialContextDto, SocialContextRequest,
};

use crate::domain::{BbsError, Domain};

impl Domain {
    pub(crate) async fn context(
        &self,
        request: SocialContextRequest,
    ) -> Result<SocialContextDto, BbsError> {
        let user_id = request.user_id.unwrap_or_else(|| "anonymous".to_string());
        Ok(self.repository.context(&user_id).await?)
    }

    pub(crate) async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<bookway_api::SocialVisibilityDto, BbsError> {
        validate_user_id(user_id)?;
        Ok(self.repository.visibility_context(user_id).await?)
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

    pub(crate) async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<RouteParticipationDto>, BbsError> {
        validate_user_id(user_id)?;
        Ok(self.repository.list_route_participations(user_id).await?)
    }

    pub(crate) async fn route_context(
        &self,
        user_id: &str,
        route_ids: Vec<String>,
    ) -> Result<RouteParticipationContextDto, BbsError> {
        validate_user_id(user_id)?;
        if route_ids.len() > 500 {
            return Err(BbsError::Validation("单次最多查询 500 条路线".to_string()));
        }
        let mut route_ids = route_ids
            .into_iter()
            .map(|route_id| route_id.trim().to_string())
            .collect::<Vec<_>>();
        if route_ids.iter().any(|route_id| route_id.is_empty()) {
            return Err(BbsError::Validation("路线 ID 不能为空".to_string()));
        }
        route_ids.sort();
        route_ids.dedup();
        Ok(self.repository.route_context(user_id, &route_ids).await?)
    }

    pub(crate) async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        request: SetRouteParticipationRequest,
    ) -> Result<RouteParticipationStateDto, BbsError> {
        validate_user_id(user_id)?;
        validate_identifier("路线 ID", route_id)?;
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
            .repository
            .set_route_participation(
                user_id,
                route_id,
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

    use bookway_api::SocialEdgeTypeDto;
    use tokio::task::JoinSet;

    use super::*;
    use crate::{conf::Config, datasource::MemoryBbsRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
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

    #[tokio::test]
    async fn visibility_context_hides_outgoing_mutes_and_blocks_in_both_directions() {
        let domain = domain();
        for (source, target, edge) in [
            ("viewer", "author-blocked", SocialEdgeTypeDto::Block),
            ("viewer", "author-muted", SocialEdgeTypeDto::Mute),
            ("author-inbound", "viewer", SocialEdgeTypeDto::Block),
        ] {
            domain
                .set_edge(source, target, FollowRequest { edge, active: true })
                .await
                .expect("social edge");
        }

        let visibility = domain
            .visibility_context("viewer")
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
        let request = SetRouteParticipationRequest {
            active: true,
            private_journey_id: Some("journey-a".to_string()),
            intent_version: None,
        };
        let first = domain
            .set_route_participation("user-a", "route-a", request.clone())
            .await
            .expect("first join");
        let retry = domain
            .set_route_participation("user-a", "route-a", request)
            .await
            .expect("idempotent retry");
        domain
            .set_route_participation(
                "user-b",
                "route-a",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: None,
                    intent_version: None,
                },
            )
            .await
            .expect("second participant");

        assert_eq!(first.joined_at, retry.joined_at);
        assert_eq!(retry.participant_count, 1);
        let context = domain
            .route_context("user-a", vec!["route-a".to_string()])
            .await
            .expect("route context");
        assert_eq!(context.joined_route_ids, vec!["route-a"]);
        assert_eq!(context.participant_counts.get("route-a"), Some(&2));
    }

    #[tokio::test]
    async fn leaving_route_removes_it_from_context() {
        let domain = domain();
        domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: None,
                    intent_version: None,
                },
            )
            .await
            .expect("join");
        let state = domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: false,
                    private_journey_id: None,
                    intent_version: None,
                },
            )
            .await
            .expect("leave");

        assert!(!state.joined);
        assert_eq!(state.participant_count, 0);
        assert!(
            domain
                .list_route_participations("user-a")
                .await
                .expect("participations")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn stale_intent_cannot_overwrite_a_newer_leave() {
        let domain = domain();
        let left = domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: false,
                    private_journey_id: None,
                    intent_version: Some(2),
                },
            )
            .await
            .expect("newer leave");
        let stale_join = domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: Some("journey-a".to_string()),
                    intent_version: Some(1),
                },
            )
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
                    .set_route_participation(
                        &format!("user-{index}"),
                        "route-hot",
                        SetRouteParticipationRequest {
                            active: true,
                            private_journey_id: None,
                            intent_version: Some(1),
                        },
                    )
                    .await
            });
        }
        while let Some(result) = joins.join_next().await {
            result.expect("join task").expect("join hot route");
        }

        let context = domain
            .route_context("observer", vec!["route-hot".to_string()])
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
                    .set_route_participation(
                        "user-a",
                        "route-hot",
                        SetRouteParticipationRequest {
                            active: true,
                            private_journey_id: Some("journey-a".to_string()),
                            intent_version: Some(1),
                        },
                    )
                    .await
            });
        }
        while let Some(result) = retries.join_next().await {
            result.expect("retry task").expect("retry join");
        }

        let state = domain
            .set_route_participation(
                "user-a",
                "route-hot",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: Some("journey-a".to_string()),
                    intent_version: Some(1),
                },
            )
            .await
            .expect("read state through retry");
        let context = domain
            .route_context("user-a", vec!["route-hot".to_string()])
            .await
            .expect("route context");
        assert_eq!(state.participant_count, 1);
        assert_eq!(context.participant_counts.get("route-hot"), Some(&1));
    }

    #[tokio::test]
    async fn a_newer_rejoin_increments_the_count_once() {
        let domain = domain();
        domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: false,
                    private_journey_id: None,
                    intent_version: Some(2),
                },
            )
            .await
            .expect("leave route");
        let stale = domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: Some("journey-stale".to_string()),
                    intent_version: Some(1),
                },
            )
            .await
            .expect("ignore stale join");
        let rejoined = domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: Some("journey-current".to_string()),
                    intent_version: Some(3),
                },
            )
            .await
            .expect("rejoin route");
        let retry = domain
            .set_route_participation(
                "user-a",
                "route-a",
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: Some("journey-current".to_string()),
                    intent_version: Some(3),
                },
            )
            .await
            .expect("retry rejoin");

        assert!(!stale.joined);
        assert!(rejoined.joined);
        assert_eq!(rejoined.participant_count, 1);
        assert_eq!(retry.participant_count, 1);
    }
}
