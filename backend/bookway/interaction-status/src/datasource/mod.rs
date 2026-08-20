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

use super::api::pb;

#[async_trait]
pub(crate) trait InteractionStatusRepository: Send + Sync {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError>;
    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError>;
}

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("interaction context cache refresh is in progress")]
    CachePeerRefresh,
}

pub(crate) struct MemoryInteractionStatusRepository {
    reactions: RwLock<HashSet<(String, String, i32)>>,
}

impl MemoryInteractionStatusRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            reactions: RwLock::new(HashSet::from([(
                "demo-user".to_string(),
                "post-reading".to_string(),
                pb::ReactionType::Like as i32,
            )])),
        }
    }
}

#[async_trait]
impl InteractionStatusRepository for MemoryInteractionStatusRepository {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError> {
        let reactions = self.reactions.read().await;
        Ok(pb::ReactionContext {
            liked_post_ids: matching_post_ids(
                &reactions,
                user_id,
                post_ids,
                pb::ReactionType::Like as i32,
            ),
            bookmarked_post_ids: matching_post_ids(
                &reactions,
                user_id,
                post_ids,
                pb::ReactionType::Bookmark as i32,
            ),
            hidden_post_ids: matching_post_ids(
                &reactions,
                user_id,
                post_ids,
                pb::ReactionType::Hide as i32,
            ),
        })
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError> {
        let mut reactions = self.reactions.write().await;
        let key = (user_id.to_string(), post_id.to_string(), reaction);
        if active {
            reactions.insert(key);
        } else {
            reactions.remove(&key);
        }
        let count = reactions
            .iter()
            .filter(|(_, target, kind)| target == post_id && *kind == reaction)
            .count() as u64;
        Ok(pb::Reaction {
            target_id: post_id.to_string(),
            target_type: "post".to_string(),
            reaction,
            active,
            count,
        })
    }
}

pub(crate) struct PostgresInteractionStatusRepository {
    pool: sqlx::PgPool,
}

impl PostgresInteractionStatusRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InteractionStatusRepository for PostgresInteractionStatusRepository {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError> {
        if post_ids.is_empty() {
            return Ok(pb::ReactionContext {
                liked_post_ids: Vec::new(),
                bookmarked_post_ids: Vec::new(),
                hidden_post_ids: Vec::new(),
            });
        }
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_id, reaction_type FROM reactions WHERE user_id = $1 AND target_type = 'post' AND target_id = ANY($2) AND deleted_at IS NULL",
        ).bind(user_id).bind(post_ids).fetch_all(&self.pool).await.map_err(RepositoryError::Database)?;
        let mut result = pb::ReactionContext {
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
            hidden_post_ids: Vec::new(),
        };
        for (target, kind) in rows {
            match kind.as_str() {
                "like" => result.liked_post_ids.push(target),
                "bookmark" => result.bookmarked_post_ids.push(target),
                "hide" => result.hidden_post_ids.push(target),
                _ => {}
            }
        }
        Ok(result)
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError> {
        let kind = match pb::ReactionType::try_from(reaction).ok() {
            Some(pb::ReactionType::Like) => "like",
            Some(pb::ReactionType::Bookmark) => "bookmark",
            Some(pb::ReactionType::Hide) => "hide",
            None => return Ok(pb::Reaction::default()),
        };
        if active {
            sqlx::query("INSERT INTO reactions (user_id,target_type,target_id,reaction_type,deleted_at) VALUES ($1,'post',$2,$3,NULL) ON CONFLICT (user_id,target_type,target_id,reaction_type) DO UPDATE SET deleted_at = NULL")
                .bind(user_id).bind(post_id).bind(kind).execute(&self.pool).await.map_err(RepositoryError::Database)?;
        } else {
            sqlx::query("UPDATE reactions SET deleted_at = now() WHERE user_id=$1 AND target_type='post' AND target_id=$2 AND reaction_type=$3 AND deleted_at IS NULL")
                .bind(user_id).bind(post_id).bind(kind).execute(&self.pool).await.map_err(RepositoryError::Database)?;
        }
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM reactions WHERE target_type='post' AND target_id=$1 AND reaction_type=$2 AND deleted_at IS NULL")
            .bind(post_id).bind(kind).fetch_one(&self.pool).await.map_err(RepositoryError::Database)?;
        Ok(pb::Reaction {
            target_id: post_id.to_string(),
            target_type: "post".to_string(),
            reaction,
            active,
            count: count.max(0) as u64,
        })
    }
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

/// Redis accelerates repeated reaction-context reads. The wrapped repository
/// remains the source of truth and is always used for mutations and misses.
pub(crate) struct CachedInteractionStatusRepository {
    inner: Arc<dyn InteractionStatusRepository>,
    redis: Option<ConnectionManager>,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

impl CachedInteractionStatusRepository {
    pub(crate) fn new(
        inner: Arc<dyn InteractionStatusRepository>,
        redis: Option<ConnectionManager>,
    ) -> Self {
        Self {
            inner,
            redis,
            miss_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn cached_context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError> {
        let cache_key = context_cache_key(user_id, post_ids);
        if let Some(value) = self.load_context(user_id, &cache_key).await {
            return Ok(value);
        }

        let _local = self.miss_lock(&cache_key).await;
        if let Some(value) = self.load_context(user_id, &cache_key).await {
            return Ok(value);
        }

        let lease = self
            .refresh_lock(&format!("bookway:interaction-status:refresh:{cache_key}"))
            .await;
        if matches!(lease, RefreshLeaseDecision::Peer) {
            if let Some(value) = self.load_context(user_id, &cache_key).await {
                return Ok(value);
            }
            return Err(RepositoryError::CachePeerRefresh);
        }

        let version_key = context_version_key(user_id);
        let version = self.cache_version(&version_key).await;
        let result = self.inner.context(user_id, post_ids).await;
        if let (Some(version), Ok(value)) = (version, result.as_ref()) {
            self.store_context(&version_key, &cache_key, version, value)
                .await;
        }
        if let RefreshLeaseDecision::Owned(Some(lease)) = lease {
            lease.release().await;
        }
        result
    }

    async fn miss_lock(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.miss_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                locks.insert(key.to_string(), Arc::downgrade(&lock));
                lock
            }
        };
        lock.lock_owned().await
    }

    async fn refresh_lock(&self, key: &str) -> RefreshLeaseDecision {
        let Some(mut manager) = self.redis.clone() else {
            return RefreshLeaseDecision::Owned(None);
        };
        let token = format!("{}-{}", std::process::id(), uuid::Uuid::now_v7());
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(CONTEXT_REFRESH_LOCK_WAIT_MS);
        loop {
            let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
                .arg(key)
                .arg(&token)
                .arg("NX")
                .arg("PX")
                .arg(CONTEXT_REFRESH_LOCK_TTL_MS)
                .query_async(&mut manager)
                .await;
            match result {
                Ok(Some(_)) => {
                    return RefreshLeaseDecision::Owned(Some(RedisRefreshLease {
                        manager,
                        key: key.to_string(),
                        token,
                    }));
                }
                Ok(None) if tokio::time::Instant::now() < deadline => {
                    sleep(Duration::from_millis(CONTEXT_REFRESH_LOCK_POLL_MS)).await;
                }
                Ok(None) => {
                    tracing::debug!(
                        key,
                        "interaction context refresh lease held by another instance"
                    );
                    return RefreshLeaseDecision::Peer;
                }
                Err(error) => {
                    tracing::debug!(%error, key, "interaction context refresh lease unavailable; using repository");
                    return RefreshLeaseDecision::Owned(None);
                }
            }
        }
    }

    async fn cache_version(&self, key: &str) -> Option<u64> {
        let mut manager = self.redis.clone()?;
        let result: redis::RedisResult<Option<String>> =
            redis::cmd("GET").arg(key).query_async(&mut manager).await;
        match result {
            Ok(Some(value)) => value.parse().ok(),
            Ok(None) => Some(0),
            Err(error) => {
                tracing::debug!(%error, key, "interaction context cache version read degraded");
                None
            }
        }
    }

    async fn load_context(&self, user_id: &str, cache_key: &str) -> Option<pb::ReactionContext> {
        let mut manager = self.redis.clone()?;
        let version_key = context_version_key(user_id);
        let result: redis::RedisResult<Vec<Option<Vec<u8>>>> = redis::cmd("MGET")
            .arg(version_key)
            .arg(cache_key)
            .query_async(&mut manager)
            .await;
        match result {
            Ok(values) => {
                let version = values
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(|value| std::str::from_utf8(value).ok())
                    .map_or(Some(0), |value| value.parse::<u64>().ok())?;
                let payload = values.get(1).and_then(Option::as_ref)?;
                if payload.len() < std::mem::size_of::<u64>() {
                    return None;
                }
                let stored_version = u64::from_be_bytes(
                    payload[..std::mem::size_of::<u64>()]
                        .try_into()
                        .unwrap_or_default(),
                );
                if stored_version != version {
                    return None;
                }
                pb::ReactionContext::decode(&payload[std::mem::size_of::<u64>()..]).ok()
            }
            Err(error) => {
                tracing::debug!(%error, "interaction context cache read degraded");
                None
            }
        }
    }

    async fn store_context(
        &self,
        version_key: &str,
        cache_key: &str,
        version: u64,
        value: &pb::ReactionContext,
    ) {
        let Some(mut manager) = self.redis.clone() else {
            return;
        };
        let mut payload = version.to_be_bytes().to_vec();
        payload.extend_from_slice(&value.encode_to_vec());
        let result: redis::RedisResult<i32> = redis::Script::new(STORE_IF_VERSION_UNCHANGED)
            .key(version_key)
            .key(cache_key)
            .arg(version.to_string())
            .arg(payload)
            .arg(CONTEXT_CACHE_TTL_SECONDS)
            .arg(CONTEXT_VERSION_TTL_SECONDS)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::debug!(%error, "interaction context cache write degraded");
        }
    }

    async fn invalidate(&self, user_id: &str) {
        let Some(mut manager) = self.redis.clone() else {
            return;
        };
        let result: redis::RedisResult<i32> = redis::Script::new(INVALIDATE_CONTEXT)
            .key(context_version_key(user_id))
            .arg(CONTEXT_VERSION_TTL_SECONDS)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "interaction context cache invalidation degraded");
        }
    }
}

#[async_trait]
impl InteractionStatusRepository for CachedInteractionStatusRepository {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError> {
        self.cached_context(user_id, post_ids).await
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError> {
        let result = self
            .inner
            .set_reaction(user_id, post_id, reaction, active)
            .await?;
        self.invalidate(user_id).await;
        Ok(result)
    }
}

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
    async fn cached_repository_falls_back_without_redis() {
        let repository = CachedInteractionStatusRepository::new(
            Arc::new(MemoryInteractionStatusRepository::seeded()),
            None,
        );
        let context = repository
            .context("demo-user", &["post-reading".to_string()])
            .await
            .expect("repository fallback should work");
        assert_eq!(context.liked_post_ids, ["post-reading"]);
    }
}
