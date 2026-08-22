use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use prost::Message;
use redis::aio::ConnectionManager;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    sync::{Mutex, RwLock},
    time::sleep,
};

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("blocked users cannot follow each other")]
    BlockedRelationship,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("timestamp formatting failed: {0}")]
    Timestamp(#[from] time::error::Format),
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

const RELATIONSHIP_CACHE_TTL_SECONDS: usize = 30;
// The version must outlive a cache entry so a stale payload cannot become
// valid after a write. It still expires to avoid one permanent Redis key per
// historical user.
const RELATIONSHIP_VERSION_TTL_SECONDS: usize = 120;
const RELATIONSHIP_REFRESH_LOCK_TTL_MS: usize = 2_000;
const RELATIONSHIP_REFRESH_LOCK_WAIT_MS: u64 = 80;
const RELATIONSHIP_REFRESH_LOCK_POLL_MS: u64 = 10;
const RELEASE_REFRESH_LOCK: &str = r#"
if redis.call('get', KEYS[1]) == ARGV[1] then
  return redis.call('del', KEYS[1])
end
return 0
"#;
const STORE_IF_VERSION_UNCHANGED: &str = r#"
local current = redis.call('get', KEYS[1])
if not current then
  current = '0'
  redis.call('set', KEYS[1], current, 'EX', ARGV[4])
end
if current ~= ARGV[1] then
  return 0
end
redis.call('set', KEYS[2], ARGV[2], 'EX', ARGV[3])
redis.call('expire', KEYS[1], ARGV[4])
return 1
"#;
const INVALIDATE_CACHE: &str = r#"
redis.call('incr', KEYS[1])
redis.call('expire', KEYS[1], ARGV[1])
return redis.call('del', KEYS[2])
"#;

struct RedisRefreshLease {
    manager: ConnectionManager,
    key: String,
    token: String,
}

impl RedisRefreshLease {
    async fn release(self) {
        let mut manager = self.manager;
        let result: redis::RedisResult<i32> = redis::Script::new(RELEASE_REFRESH_LOCK)
            .key(self.key)
            .arg(self.token)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::debug!(%error, "bbs relationship refresh lease release degraded");
        }
    }
}

enum RefreshLeaseDecision {
    Owned(Option<RedisRefreshLease>),
    Peer,
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

fn relationship_cache_key(kind: &str, user_id: &str) -> String {
    format!("bookway:bbs:{kind}:{}", relationship_identity(user_id))
}

fn relationship_version_key(kind: &str, user_id: &str) -> String {
    format!(
        "bookway:bbs:{kind}:version:{}",
        relationship_identity(user_id)
    )
}

fn relationship_refresh_key(kind: &str, user_id: &str) -> String {
    format!(
        "bookway:bbs:{kind}:refresh:{}",
        relationship_identity(user_id)
    )
}

fn command_is_accepted(current_version: u64, incoming_version: Option<u64>) -> bool {
    incoming_version
        .map(|version| version >= current_version)
        .unwrap_or(current_version == 0)
}

fn format_timestamp(value: time::OffsetDateTime) -> Result<String, time::error::Format> {
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

fn targets(
    edges: &HashSet<(String, String, pb::SocialEdgeType)>,
    user_id: &str,
    edge_type: pb::SocialEdgeType,
) -> Vec<String> {
    edges
        .iter()
        .filter(|(source, _, edge)| source == user_id && *edge == edge_type)
        .map(|(_, target, _)| target.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{BbsDao, CachedBbsDao, MemoryBbsDao, ordered_social_pair, relationship_cache_key};

    #[test]
    fn social_pair_lock_has_one_key_for_both_directions() {
        assert_eq!(
            ordered_social_pair("user-a", "user-b"),
            ordered_social_pair("user-b", "user-a")
        );
    }

    #[test]
    fn relationship_cache_keys_do_not_expose_user_ids() {
        let first = relationship_cache_key("context", "user-a");
        let second = relationship_cache_key("context", "user-b");
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
