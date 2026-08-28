use crate::{
    api::pb,
    datasource::{format_timestamp, FollowedEdge, KeysetCursor, PeerEdge},
};

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

    pub(crate) async fn list_followers(
        &self,
        request: pb::ListFollowersRequest,
    ) -> Result<pb::FollowerPage, BbsError> {
        validate_user_id(&request.user_id)?;
        let before = decode_keyset_cursor(&request.cursor)?;
        let limit = if request.limit == 0 {
            DEFAULT_FOLLOWER_PAGE_LIMIT
        } else {
            request.limit.min(MAX_FOLLOWER_PAGE_LIMIT)
        };
        let items = self
            .dao
            .list_followers(&request.user_id, before, limit)
            .await?;
        // A full page implies more may follow; the last row becomes the resume
        // key. One extra empty page at the tail is the honest keyset cost.
        let next_cursor = (items.len() as u32 == limit).then(|| {
            items
                .last()
                .map(|edge| encode_keyset_cursor(&edge.follower_id, edge.followed_at))
        });
        let mut followers = Vec::with_capacity(items.len());
        for FollowedEdge {
            follower_id,
            followed_at,
        } in items
        {
            followers.push(pb::Follower {
                user_id: follower_id,
                followed_at: format_timestamp(followed_at)
                    .map_err(|_| BbsError::Validation("时间戳格式化失败".to_string()))?,
            });
        }
        Ok(pb::FollowerPage {
            items: followers,
            next_cursor: next_cursor.flatten(),
        })
    }

    /// Co-walkers of one route, resolved from public participation facts and
    /// filtered by the viewer's visibility context. That read is fail-closed:
    /// when the visibility layer cannot prove freshness it errors out instead
    /// of serving an unknown relationship as "visible".
    pub(crate) async fn list_route_peers(
        &self,
        request: pb::ListRoutePeersRequest,
    ) -> Result<pb::RoutePeerPage, BbsError> {
        validate_user_id(&request.viewer_id)?;
        validate_identifier("路线 ID", &request.route_id)?;
        let before = decode_keyset_cursor(&request.cursor)?;
        let limit = if request.limit == 0 {
            DEFAULT_FOLLOWER_PAGE_LIMIT
        } else {
            request.limit.min(MAX_FOLLOWER_PAGE_LIMIT)
        };
        let excluded_user_ids = self
            .dao
            .visibility_context(&request.viewer_id)
            .await?
            .excluded_author_ids;
        let items = self
            .dao
            .list_route_peers(
                &request.route_id,
                &request.viewer_id,
                &excluded_user_ids,
                before,
                limit,
            )
            .await?;
        let next_cursor = (items.len() as u32 == limit).then(|| {
            items
                .last()
                .map(|peer| encode_keyset_cursor(&peer.user_id, peer.joined_at))
        });
        let mut peers = Vec::with_capacity(items.len());
        for PeerEdge { user_id, joined_at } in items {
            peers.push(pb::RoutePeer {
                user_id,
                joined_at: format_timestamp(joined_at)
                    .map_err(|_| BbsError::Validation("时间戳格式化失败".to_string()))?,
            });
        }
        Ok(pb::RoutePeerPage {
            items: peers,
            next_cursor: next_cursor.flatten(),
        })
    }

    pub(crate) async fn get_social_stats(
        &self,
        request: pb::SocialStatsRequest,
    ) -> Result<pb::SocialStats, BbsError> {
        validate_user_id(&request.user_id)?;
        let (followers, following) = self.dao.social_stats(&request.user_id).await?;
        Ok(pb::SocialStats {
            followers,
            following,
        })
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
        // Participant counts are public aggregate facts, so an empty user_id
        // is a legitimate anonymous counts-only read; it only skips the
        // joined-route enrichment that requires an identity.
        let user_id = request.user_id.trim().to_string();
        if !user_id.is_empty() {
            validate_user_id(&user_id)?;
        }
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
        Ok(self.dao.route_context(&user_id, &route_ids).await?)
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

const DEFAULT_FOLLOWER_PAGE_LIMIT: u32 = 50;
const MAX_FOLLOWER_PAGE_LIMIT: u32 = 200;

/// `{unix_nanos}.{user_id}` — resumable ordering by `(time, id)` descending.
/// `split_once` keeps ids that themselves contain dots intact. Shared by the
/// follower and co-walker pages, which use the same ordering discipline. The
/// full-nanosecond stamp matters: truncating to milliseconds would swallow the
/// rows sharing the truncated millisecond on the next page.
fn encode_keyset_cursor(id: &str, at: time::OffsetDateTime) -> String {
    // Nanoseconds since the epoch fit an i64 until year 2262.
    let nanos = i64::try_from(at.unix_timestamp_nanos()).unwrap_or(i64::MAX);
    format!("{nanos}.{id}")
}

fn decode_keyset_cursor(cursor: &str) -> Result<Option<KeysetCursor>, BbsError> {
    let trimmed = cursor.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let Some((nanos, id)) = trimmed.split_once('.') else {
        return Err(BbsError::Validation("列表游标无效".to_string()));
    };
    let nanos: i64 = nanos
        .parse()
        .map_err(|_| BbsError::Validation("列表游标无效".to_string()))?;
    if nanos < 0 {
        // Follow and join times are never before the epoch; reject rather than
        // serve a cursor pointing into 1969.
        return Err(BbsError::Validation("列表游标无效".to_string()));
    }
    let at = time::OffsetDateTime::from_unix_timestamp_nanos(nanos as i128)
        .map_err(|_| BbsError::Validation("列表游标无效".to_string()))?;
    validate_user_id(id)?;
    Ok(Some(KeysetCursor {
        at,
        id: id.to_string(),
    }))
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
    async fn anonymous_route_context_returns_counts_without_joined_enrichment() {
        let domain = domain();
        let joined = domain
            .set_route_participation(pb::RouteParticipationRequest {
                user_id: "walker".to_string(),
                route_id: "route-a".to_string(),
                active: true,
                private_journey_id: None,
                intent_version: None,
            })
            .await
            .expect("seed one joined route");

        let anonymous = domain
            .route_context(pb::RouteContextRequest {
                user_id: String::new(),
                route_ids: vec!["route-a".to_string()],
            })
            .await
            .expect("anonymous context read");
        assert_eq!(anonymous.participant_counts.get("route-a"), Some(&1));
        assert!(
            anonymous.joined_route_ids.is_empty(),
            "anonymous reads carry no identity-bound enrichment"
        );

        let identified = domain
            .route_context(pb::RouteContextRequest {
                user_id: "walker".to_string(),
                route_ids: vec!["route-a".to_string()],
            })
            .await
            .expect("identified context read");
        assert_eq!(
            identified.joined_route_ids,
            vec!["route-a".to_string()],
            "identified reads keep joined-route enrichment (journey {})",
            joined.private_journey_id.as_deref().unwrap_or_default()
        );
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

    #[tokio::test]
    async fn follower_pages_walk_newest_first_and_resume_without_duplicates() {
        let domain = domain();
        for follower in ["user-a", "user-b"] {
            domain
                .set_edge(pb::SetEdgeRequest {
                    user_id: follower.to_string(),
                    target_user_id: "author-changfeng".to_string(),
                    edge: pb::SocialEdgeType::Follow as i32,
                    active: true,
                })
                .await
                .expect("follow");
        }

        let first = domain
            .list_followers(pb::ListFollowersRequest {
                user_id: "author-changfeng".to_string(),
                cursor: String::new(),
                limit: 2,
            })
            .await
            .expect("first page");
        let names: Vec<_> = first.items.iter().map(|f| f.user_id.as_str()).collect();
        // Insertion order also settles follower-id ordering on clock ties.
        assert_eq!(names, vec!["user-b", "user-a"]);
        let resume = first.next_cursor.clone().expect("resume cursor");

        let second = domain
            .list_followers(pb::ListFollowersRequest {
                user_id: "author-changfeng".to_string(),
                cursor: resume.clone(),
                limit: 2,
            })
            .await
            .expect("second page");
        let rest: Vec<_> = second.items.iter().map(|f| f.user_id.as_str()).collect();
        assert_eq!(rest, vec!["demo-user"]);
        assert!(second.next_cursor.is_none());

        // Re-walking with the same cursor must produce the same page.
        let repeat = domain
            .list_followers(pb::ListFollowersRequest {
                user_id: "author-changfeng".to_string(),
                cursor: resume,
                limit: 2,
            })
            .await
            .expect("repeat page");
        assert_eq!(
            repeat
                .items
                .iter()
                .map(|f| f.user_id.as_str())
                .collect::<Vec<_>>(),
            rest
        );
    }

    #[tokio::test]
    async fn social_stats_count_both_edge_directions() {
        let domain = domain();
        for (source, target) in [
            ("user-a", "author-changfeng"),
            ("author-changfeng", "creator-north"),
        ] {
            domain
                .set_edge(pb::SetEdgeRequest {
                    user_id: source.to_string(),
                    target_user_id: target.to_string(),
                    edge: pb::SocialEdgeType::Follow as i32,
                    active: true,
                })
                .await
                .expect("follow");
        }

        let stats = domain
            .get_social_stats(pb::SocialStatsRequest {
                user_id: "author-changfeng".to_string(),
            })
            .await
            .expect("stats");
        assert_eq!((stats.followers, stats.following), (2, 1));

        // Unfollowing drops the count immediately.
        domain
            .set_edge(pb::SetEdgeRequest {
                user_id: "user-a".to_string(),
                target_user_id: "author-changfeng".to_string(),
                edge: pb::SocialEdgeType::Follow as i32,
                active: false,
            })
            .await
            .expect("unfollow");
        let stats = domain
            .get_social_stats(pb::SocialStatsRequest {
                user_id: "author-changfeng".to_string(),
            })
            .await
            .expect("stats after unfollow");
        assert_eq!((stats.followers, stats.following), (1, 1));
    }

    #[test]
    fn follower_cursor_round_trips_ids_with_dots() {
        let encoded = encode_keyset_cursor(
            "user.dot",
            time::OffsetDateTime::from_unix_timestamp_nanos(1_700_000_123_456_000_000)
                .expect("timestamp"),
        );
        let decoded = decode_keyset_cursor(&encoded).expect("valid cursor");
        assert_eq!(
            decoded,
            Some(KeysetCursor {
                at: time::OffsetDateTime::from_unix_timestamp_nanos(1_700_000_123_456_000_000)
                    .expect("timestamp"),
                id: "user.dot".to_string(),
            })
        );
    }

    #[test]
    fn blank_or_malformed_follower_cursors_are_rejected() {
        assert!(decode_keyset_cursor("")
            .expect("blank cursor is just page one")
            .is_none());
        for bad in ["garbage", "-5.user", "notanumber.user"] {
            assert!(decode_keyset_cursor(bad).is_err(), "cursor {bad:?} passes");
        }
    }

    #[tokio::test]
    async fn route_peers_exclude_the_viewer_and_visibility_blocks() {
        let domain = domain();
        for participant in ["user-a", "user-b"] {
            domain
                .set_route_participation(pb::RouteParticipationRequest {
                    user_id: participant.to_string(),
                    route_id: "route-hot".to_string(),
                    active: true,
                    private_journey_id: None,
                    intent_version: None,
                })
                .await
                .expect("join");
        }

        let page = domain
            .list_route_peers(pb::ListRoutePeersRequest {
                viewer_id: "viewer".to_string(),
                route_id: "route-hot".to_string(),
                cursor: String::new(),
                limit: 10,
            })
            .await
            .expect("peers");
        let names: Vec<_> = page.items.iter().map(|peer| peer.user_id.as_str()).collect();
        assert_eq!(names, vec!["user-b", "user-a"]);
        assert!(page.next_cursor.is_none());

        // The viewer never appears in their own co-walker list.
        let own = domain
            .list_route_peers(pb::ListRoutePeersRequest {
                viewer_id: "user-a".to_string(),
                route_id: "route-hot".to_string(),
                cursor: String::new(),
                limit: 10,
            })
            .await
            .expect("own peers");
        assert_eq!(
            own.items.iter().map(|peer| peer.user_id.as_str()).collect::<Vec<_>>(),
            vec!["user-b"]
        );

        // A block (either direction) hides the relationship fail-closed.
        domain
            .set_edge(pb::SetEdgeRequest {
                user_id: "viewer".to_string(),
                target_user_id: "user-a".to_string(),
                edge: pb::SocialEdgeType::Block as i32,
                active: true,
            })
            .await
            .expect("block");
        let filtered = domain
            .list_route_peers(pb::ListRoutePeersRequest {
                viewer_id: "viewer".to_string(),
                route_id: "route-hot".to_string(),
                cursor: String::new(),
                limit: 10,
            })
            .await
            .expect("filtered peers");
        assert_eq!(
            filtered
                .items
                .iter()
                .map(|peer| peer.user_id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-b"]
        );
    }

    #[tokio::test]
    async fn route_peers_keyset_paging_is_stable() {
        let domain = domain();
        for index in 0..3 {
            domain
                .set_route_participation(pb::RouteParticipationRequest {
                    user_id: format!("user-{index}"),
                    route_id: "route-paged".to_string(),
                    active: true,
                    private_journey_id: None,
                    intent_version: None,
                })
                .await
                .expect("join");
        }
        let first = domain
            .list_route_peers(pb::ListRoutePeersRequest {
                viewer_id: "viewer".to_string(),
                route_id: "route-paged".to_string(),
                cursor: String::new(),
                limit: 2,
            })
            .await
            .expect("first page");
        assert_eq!(first.items.len(), 2);
        let resume = first.next_cursor.expect("resume cursor");

        let second = domain
            .list_route_peers(pb::ListRoutePeersRequest {
                viewer_id: "viewer".to_string(),
                route_id: "route-paged".to_string(),
                cursor: resume.clone(),
                limit: 2,
            })
            .await
            .expect("second page");
        assert_eq!(second.items.len(), 1);
        assert!(second.next_cursor.is_none());

        let repeat = domain
            .list_route_peers(pb::ListRoutePeersRequest {
                viewer_id: "viewer".to_string(),
                route_id: "route-paged".to_string(),
                cursor: resume,
                limit: 2,
            })
            .await
            .expect("repeat page");
        assert_eq!(
            repeat
                .items
                .iter()
                .map(|peer| peer.user_id.clone())
                .collect::<Vec<_>>(),
            second
                .items
                .iter()
                .map(|peer| peer.user_id.clone())
                .collect::<Vec<_>>()
        );
    }
}
