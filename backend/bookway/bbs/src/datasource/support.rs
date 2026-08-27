use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::Arc,
};

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("blocked users cannot follow each other")]
    BlockedRelationship,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("timestamp formatting failed: {0}")]
    Timestamp(#[from] time::error::Format),
    #[error("stored timestamp failed to parse: {0}")]
    TimestampParse(#[from] time::error::Parse),
    #[error("relationship cache refresh is in progress")]
    CachePeerRefresh,
}

#[async_trait]
pub(crate) trait BbsDao: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, DaoError>;
    async fn visibility_context(&self, user_id: &str) -> Result<pb::SocialVisibility, DaoError>;
    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, DaoError>;
    /// Keyset page of a user's followers: the newest follows first, resuming
    /// strictly after `before` when present. A structured cursor (not an
    /// offset) keeps pages stable while new follows stream in at the head.
    async fn list_followers(
        &self,
        user_id: &str,
        before: Option<KeysetCursor>,
        limit: u32,
    ) -> Result<Vec<FollowedEdge>, DaoError>;
    /// Keyset page of a route's public co-walkers: active participants other
    /// than the viewer, minus the viewer's visibility exclusions (blocks in
    /// either direction plus outgoing mutes, resolved by the domain).
    async fn list_route_peers(
        &self,
        route_id: &str,
        viewer_id: &str,
        excluded_user_ids: &[String],
        before: Option<KeysetCursor>,
        limit: u32,
    ) -> Result<Vec<PeerEdge>, DaoError>;
    /// Live follower/following counts for one user. Cheap enough to serve
    /// without a dedicated counter table; caching happens in the wrapper.
    async fn social_stats(&self, user_id: &str) -> Result<(u64, u64), DaoError>;
    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, DaoError>;
    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, DaoError>;
    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, DaoError>;
}

/// Resume position for a keyset page over `(time, id)` pairs: everything at or
/// before this instant and id has already been served.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeysetCursor {
    pub(crate) at: time::OffsetDateTime,
    pub(crate) id: String,
}

/// One inbound follow edge with its commit time.
#[derive(Debug, Clone)]
pub(crate) struct FollowedEdge {
    pub(crate) follower_id: String,
    pub(crate) followed_at: time::OffsetDateTime,
}

/// One active route participation with its join time.
#[derive(Debug, Clone)]
pub(crate) struct PeerEdge {
    pub(crate) user_id: String,
    pub(crate) joined_at: time::OffsetDateTime,
}

const RELATIONSHIP_CACHE_TTL_SECONDS: u64 = 30;
// The version must outlive a cache entry so a stale payload cannot become
// valid after a write. It still expires to avoid one permanent Redis key per
// historical user.
const RELATIONSHIP_VERSION_TTL_SECONDS: u64 = 120;
const RELATIONSHIP_REFRESH_LOCK_TTL_MS: usize = 2_000;
const RELATIONSHIP_REFRESH_LOCK_WAIT_MS: u64 = 80;
const RELATIONSHIP_REFRESH_LOCK_POLL_MS: u64 = 10;

/// Both relationship caches stamp payloads with the user's invalidation
/// counter and serve them only while the stamp matches, so a committed block
/// or mute retires cached visibility immediately. Construction, tuning, and
/// the Redis-fail-open rules live in `bookway_cache::VersionedCache`.
fn relationship_cache<M>(
    redis: Option<ConnectionManager>,
    kind: &str,
) -> bookway_cache::VersionedCache<M>
where
    M: prost::Message + Default + Clone,
{
    bookway_cache::VersionedCache::new(
        redis,
        &format!("bookway:bbs:{kind}"),
        RELATIONSHIP_CACHE_TTL_SECONDS,
        RELATIONSHIP_VERSION_TTL_SECONDS,
    )
    .with_refresh_tuning(
        RELATIONSHIP_REFRESH_LOCK_WAIT_MS,
        RELATIONSHIP_REFRESH_LOCK_POLL_MS,
        RELATIONSHIP_REFRESH_LOCK_TTL_MS,
    )
}

pub(crate) fn relationship_context_cache(
    redis: Option<ConnectionManager>,
) -> bookway_cache::VersionedCache<pb::SocialContext> {
    relationship_cache(redis, "context")
}

pub(crate) fn relationship_visibility_cache(
    redis: Option<ConnectionManager>,
) -> bookway_cache::VersionedCache<pb::SocialVisibility> {
    relationship_cache(redis, "visibility")
}

pub(crate) fn relationship_stats_cache(
    redis: Option<ConnectionManager>,
) -> bookway_cache::VersionedCache<pb::SocialStats> {
    relationship_cache(redis, "stats")
}

/// Redis is an acceleration and coordination layer only. The wrapped
/// dao remains the source of truth for every relationship mutation and
/// for cache misses when Redis is unavailable.
fn relationship_identity(user_id: &str) -> String {
    Sha256::digest(user_id.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn command_is_accepted(current_version: u64, incoming_version: Option<u64>) -> bool {
    incoming_version
        .map(|version| version >= current_version)
        .unwrap_or(current_version == 0)
}

pub(crate) fn format_timestamp(value: time::OffsetDateTime) -> Result<String, time::error::Format> {
    value.format(&time::format_description::well_known::Rfc3339)
}

fn edge_name(edge: pb::SocialEdgeType) -> &'static str {
    match edge {
        pb::SocialEdgeType::Follow => "follow",
        pb::SocialEdgeType::Block => "block",
        pb::SocialEdgeType::Mute => "mute",
    }
}

fn ordered_social_pair<'a>(first: &'a str, second: &'a str) -> (&'a str, &'a str) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn targets<'a>(
    edges: impl Iterator<Item = &'a (String, String, pb::SocialEdgeType)>,
    user_id: &str,
    edge_type: pb::SocialEdgeType,
) -> Vec<String> {
    edges
        .filter(|(source, _, edge)| source == user_id && *edge == edge_type)
        .map(|(_, target, _)| target.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        BbsDao, CachedBbsDao, MemoryBbsDao, ordered_social_pair, relationship_identity,
    };

    #[test]
    fn social_pair_lock_has_one_key_for_both_directions() {
        assert_eq!(
            ordered_social_pair("user-a", "user-b"),
            ordered_social_pair("user-b", "user-a")
        );
    }

    #[test]
    fn relationship_cache_keys_do_not_expose_user_ids() {
        let first = relationship_identity("user-a");
        let second = relationship_identity("user-b");
        assert_ne!(first, second);
        assert!(!first.contains("user-a"));
        assert!(!second.contains("user-b"));
    }

    #[tokio::test]
    async fn relationship_cache_falls_back_to_the_dao_without_redis() {
        let dao = Arc::new(CachedBbsDao::new(Arc::new(MemoryBbsDao::seeded()), None));
        let context = dao.context("demo-user").await.expect("context");
        assert_eq!(context.followed_author_ids, vec!["author-changfeng"]);
    }
}

#[path = "memory_bbs_dao.rs"]
mod memory_bbs_dao;
pub(crate) use memory_bbs_dao::MemoryBbsDao;
#[path = "postgres_bbs_dao.rs"]
mod postgres_bbs_dao;
pub(crate) use postgres_bbs_dao::PostgresBbsDao;
#[path = "cached_bbs_dao.rs"]
mod cached_bbs_dao;
pub(crate) use cached_bbs_dao::CachedBbsDao;
