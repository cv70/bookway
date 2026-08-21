use std::{
    collections::{HashMap, HashSet},
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

const CONTEXT_CACHE_TTL_SECONDS: usize = 30;
const CONTEXT_VERSION_TTL_SECONDS: usize = 120;
const CONTEXT_REFRESH_LOCK_TTL_MS: usize = 2_000;
const CONTEXT_REFRESH_LOCK_WAIT_MS: u64 = 80;
const CONTEXT_REFRESH_LOCK_POLL_MS: u64 = 10;
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
const INVALIDATE_CONTEXT: &str = r#"
redis.call('incr', KEYS[1])
redis.call('expire', KEYS[1], ARGV[1])
return 1
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
            tracing::debug!(%error, "interaction context refresh lease release degraded");
        }
    }
}

enum RefreshLeaseDecision {
    Owned(Option<RedisRefreshLease>),
    Peer,
}

/// Redis accelerates repeated reaction-context reads. The wrapped Dao
/// remains the source of truth and is always used for mutations and misses.

fn context_version_key(user_id: &str) -> String {
    format!(
        "bookway:interaction-status:version:{}",
        hash_identifier(user_id)
    )
}

fn context_cache_key(user_id: &str, post_ids: &[String]) -> String {
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
    format!(
        "bookway:interaction-status:context:{}",
        hex_digest(hasher.finalize())
    )
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
        let key = context_cache_key("user-a", &first);
        assert_eq!(key, context_cache_key("user-a", &second));
        assert!(!key.contains("user-a"));
        assert!(!key.contains("post-a"));
    }

    #[tokio::test]
    async fn cached_Dao_falls_back_without_redis() {
        let Dao =
            CachedInteractionStatusDao::new(Arc::new(MemoryInteractionStatusDao::seeded()), None);
        let context = Dao
            .context("demo-user", &["post-reading".to_string()])
            .await
            .expect("Dao fallback should work");
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
