use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::api::pb;

#[async_trait]
pub(crate) trait InteractionStatusDao: Send + Sync {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, DaoError>;
    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, DaoError>;
}

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("interaction context cache refresh is in progress")]
    CachePeerRefresh,
}

const CONTEXT_CACHE_TTL_SECONDS: u64 = 30;
// The version must outlive a cache entry so a stale payload cannot become
// valid after a write. It still expires to avoid one permanent Redis key per
// historical user.
const CONTEXT_VERSION_TTL_SECONDS: u64 = 120;
const CONTEXT_REFRESH_LOCK_TTL_MS: usize = 2_000;
const CONTEXT_REFRESH_LOCK_WAIT_MS: u64 = 80;
const CONTEXT_REFRESH_LOCK_POLL_MS: u64 = 10;

/// Reaction-context cache with per-user invalidation counters: payloads are
/// keyed by the (user, post-set) combination but stamped against — and
/// retired by — one counter per user, so a new reaction instantly stops every
/// cached combination for that user from being served. Construction, tuning,
/// and the Redis-fail-open rules live in `bookway_cache::VersionedCache`.
pub(crate) fn reaction_context_cache(
    redis: Option<ConnectionManager>,
) -> bookway_cache::VersionedCache<pb::ReactionContext> {
    bookway_cache::VersionedCache::new_scoped(
        redis,
        "bookway:interaction-status:context",
        "bookway:interaction-status:user-ver",
        CONTEXT_CACHE_TTL_SECONDS,
        CONTEXT_VERSION_TTL_SECONDS,
    )
    .with_refresh_tuning(
        CONTEXT_REFRESH_LOCK_WAIT_MS,
        CONTEXT_REFRESH_LOCK_POLL_MS,
        CONTEXT_REFRESH_LOCK_TTL_MS,
    )
}

/// Redis accelerates repeated reaction-context reads. The wrapped dao
/// remains the source of truth and is always used for mutations and misses.
fn context_version_scope(user_id: &str) -> String {
    hash_identifier(user_id)
}

fn context_entry_key(user_id: &str, post_ids: &[String]) -> String {
    let mut canonical = post_ids.to_vec();
    canonical.sort_unstable();
    canonical.dedup();
    let mut hasher = Sha256::new();
    hasher.update(user_id.len().to_be_bytes());
    hasher.update(user_id.as_bytes());
    for post_id in canonical {
        hasher.update(post_id.len().to_be_bytes());
        hasher.update(post_id.as_bytes());
    }
    hex_digest(hasher.finalize())
}

fn hash_identifier(value: &str) -> String {
    hex_digest(Sha256::digest(value.as_bytes()))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn matching_post_ids(
    reactions: &HashSet<(String, String, i32)>,
    user_id: &str,
    post_ids: &[String],
    reaction: i32,
) -> Vec<String> {
    post_ids
        .iter()
        .filter(|post_id| reactions.contains(&(user_id.to_string(), (*post_id).clone(), reaction)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_cache_key_is_order_independent_and_hashed() {
        let first = vec!["post-a".to_string(), "post-b".to_string()];
        let second = vec!["post-b".to_string(), "post-a".to_string()];
        let key = context_entry_key("user-a", &first);
        assert_eq!(key, context_entry_key("user-a", &second));
        assert!(!key.contains("user-a"));
        assert!(!key.contains("post-a"));
    }

    #[tokio::test]
    async fn cached_dao_falls_back_without_redis() {
        let dao =
            CachedInteractionStatusDao::new(Arc::new(MemoryInteractionStatusDao::seeded()), None);
        let context = dao
            .context("demo-user", &["post-reading".to_string()])
            .await
            .expect("dao fallback should work");
        assert_eq!(context.liked_post_ids, ["post-reading"]);
    }
}

#[path = "memory_interaction_status_dao.rs"]
mod memory_interaction_status_dao;
pub(crate) use memory_interaction_status_dao::MemoryInteractionStatusDao;
#[path = "postgres_interaction_status_dao.rs"]
mod postgres_interaction_status_dao;
pub(crate) use postgres_interaction_status_dao::PostgresInteractionStatusDao;
#[path = "cached_interaction_status_dao.rs"]
mod cached_interaction_status_dao;
pub(crate) use cached_interaction_status_dao::CachedInteractionStatusDao;
