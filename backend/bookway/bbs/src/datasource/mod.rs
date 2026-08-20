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
pub(crate) enum RepositoryError {
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
pub(crate) trait BbsRepository: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError>;
    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError>;
    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError>;
    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError>;
    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError>;
    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError>;
}

pub(crate) struct MemoryBbsRepository {
    edges: RwLock<HashSet<(String, String, pb::SocialEdgeType)>>,
    route_participations: RwLock<HashMap<(String, String), pb::RouteParticipation>>,
    route_intent_versions: RwLock<HashMap<(String, String), u64>>,
}

impl MemoryBbsRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            edges: RwLock::new(HashSet::from([(
                "demo-user".to_string(),
                "author-changfeng".to_string(),
                pb::SocialEdgeType::Follow,
            )])),
            route_participations: RwLock::new(HashMap::new()),
            route_intent_versions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl BbsRepository for MemoryBbsRepository {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError> {
        let edges = self.edges.read().await;
        Ok(pb::SocialContext {
            followed_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Follow),
            blocked_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Block),
            muted_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Mute),
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
        })
    }

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError> {
        let edges = self.edges.read().await;
        let mut excluded_author_ids = edges
            .iter()
            .filter_map(|(source, target, edge)| match edge {
                pb::SocialEdgeType::Block | pb::SocialEdgeType::Mute if source == user_id => {
                    Some(target.clone())
                }
                pb::SocialEdgeType::Block if target == user_id => Some(source.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        excluded_author_ids.sort();
        excluded_author_ids.dedup();
        Ok(pb::SocialVisibility {
            excluded_author_ids,
        })
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError> {
        let mut edges = self.edges.write().await;
        let key = (user_id.to_string(), target_user_id.to_string(), edge);
        if active && edge == pb::SocialEdgeType::Follow {
            let blocked = [
                (
                    user_id.to_string(),
                    target_user_id.to_string(),
                    pb::SocialEdgeType::Block,
                ),
                (
                    target_user_id.to_string(),
                    user_id.to_string(),
                    pb::SocialEdgeType::Block,
                ),
            ]
            .iter()
            .any(|block| edges.contains(block));
            if blocked {
                return Err(RepositoryError::BlockedRelationship);
            }
        }
        if active && edge == pb::SocialEdgeType::Block {
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                pb::SocialEdgeType::Follow,
            ));
            edges.remove(&(
                target_user_id.to_string(),
                user_id.to_string(),
                pb::SocialEdgeType::Follow,
            ));
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                pb::SocialEdgeType::Mute,
            ));
        }
        if active {
            edges.insert(key);
        } else {
            edges.remove(&key);
        }
        drop(edges);
        self.context(user_id).await
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError> {
        let participations = self.route_participations.read().await;
        let mut items = participations
            .iter()
            .filter(|((_, participant_id), _)| participant_id == user_id)
            .map(|(_, participation)| participation.clone())
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.joined_at.cmp(&left.joined_at));
        Ok(items)
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError> {
        let requested = route_ids.iter().collect::<HashSet<_>>();
        let participations = self.route_participations.read().await;
        let mut context = pb::RouteParticipationContext::default();
        for (route_id, participant_id) in participations.keys() {
            if !requested.contains(route_id) {
                continue;
            }
            *context
                .participant_counts
                .entry(route_id.clone())
                .or_default() += 1;
            if participant_id == user_id {
                context.joined_route_ids.push(route_id.clone());
            }
        }
        context.joined_route_ids.sort();
        Ok(context)
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError> {
        let key = (route_id.to_string(), user_id.to_string());
        let mut versions = self.route_intent_versions.write().await;
        let mut participations = self.route_participations.write().await;
        let current_version = versions.get(&key).copied().unwrap_or_default();
        let accepted = command_is_accepted(current_version, intent_version);
        if accepted && let Some(version) = intent_version {
            versions.insert(key.clone(), version);
        }
        if accepted && active {
            let joined_at = participations
                .get(&key)
                .map(|item| item.joined_at.clone())
                .unwrap_or(format_timestamp(time::OffsetDateTime::now_utc())?);
            participations.insert(
                key.clone(),
                pb::RouteParticipation {
                    route_id: route_id.to_string(),
                    private_journey_id: private_journey_id.clone(),
                    joined_at: joined_at.clone(),
                },
            );
        } else if accepted {
            participations.remove(&key);
        }
        let participant_count = participations
            .keys()
            .filter(|(current_route_id, _)| current_route_id == route_id)
            .count() as u64;
        let participation = participations.get(&key);
        Ok(pb::RouteParticipationState {
            route_id: route_id.to_string(),
            joined: participation.is_some(),
            private_journey_id: participation.and_then(|item| item.private_journey_id.clone()),
            joined_at: participation.map(|item| item.joined_at.clone()),
            participant_count,
        })
    }
}

pub(crate) struct PostgresBbsRepository {
    pool: sqlx::PgPool,
}

impl PostgresBbsRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BbsRepository for PostgresBbsRepository {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_user_id, edge_type FROM social_edges WHERE source_user_id = $1 AND deleted_at IS NULL ORDER BY target_user_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let mut context = pb::SocialContext {
            followed_author_ids: Vec::new(),
            blocked_author_ids: Vec::new(),
            muted_author_ids: Vec::new(),
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
        };
        for (target, edge_type) in rows {
            match edge_type.as_str() {
                "follow" => context.followed_author_ids.push(target),
                "block" => context.blocked_author_ids.push(target),
                "mute" => context.muted_author_ids.push(target),
                _ => {}
            }
        }
        Ok(context)
    }

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError> {
        let excluded_author_ids = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT CASE WHEN source_user_id = $1 THEN target_user_id ELSE source_user_id END FROM social_edges WHERE deleted_at IS NULL AND ((source_user_id = $1 AND edge_type IN ('block', 'mute')) OR (target_user_id = $1 AND edge_type = 'block')) ORDER BY 1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(pb::SocialVisibility {
            excluded_author_ids,
        })
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError> {
        let edge_type = edge_name(edge);
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let (first_user_id, second_user_id) = ordered_social_pair(user_id, target_user_id);
        // A block removes follows in both directions. Serialize every mutation
        // for this user pair so a concurrent follow cannot commit after that cleanup.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::TEXT), hashtext($2::TEXT))")
            .bind(first_user_id)
            .bind(second_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        if active && edge == pb::SocialEdgeType::Follow {
            let blocked = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM social_edges WHERE edge_type = 'block' AND deleted_at IS NULL AND ((source_user_id = $1 AND target_user_id = $2) OR (source_user_id = $2 AND target_user_id = $1)))",
            )
            .bind(user_id)
            .bind(target_user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if blocked {
                return Err(RepositoryError::BlockedRelationship);
            }
        }
        if active && edge == pb::SocialEdgeType::Block {
            sqlx::query(
                "UPDATE social_edges SET deleted_at = now() WHERE deleted_at IS NULL AND ((edge_type = 'follow' AND ((source_user_id = $1 AND target_user_id = $2) OR (source_user_id = $2 AND target_user_id = $1))) OR (edge_type = 'mute' AND source_user_id = $1 AND target_user_id = $2))",
            )
            .bind(user_id)
            .bind(target_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        if active {
            sqlx::query(
                "INSERT INTO social_edges (source_user_id, target_user_id, edge_type) VALUES ($1, $2, $3) ON CONFLICT (source_user_id, target_user_id, edge_type) DO UPDATE SET deleted_at = NULL, created_at = now()",
            )
            .bind(user_id)
            .bind(target_user_id)
            .bind(edge_type)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        } else {
            sqlx::query(
                "UPDATE social_edges SET deleted_at = now() WHERE source_user_id = $1 AND target_user_id = $2 AND edge_type = $3 AND deleted_at IS NULL",
            )
            .bind(user_id)
            .bind(target_user_id)
            .bind(edge_type)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        self.context(user_id).await
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, Option<String>, time::OffsetDateTime)>(
            "SELECT route_id, private_journey_id, joined_at FROM route_participations WHERE user_id = $1 AND left_at IS NULL ORDER BY joined_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|(route_id, private_journey_id, joined_at)| {
                Ok(pb::RouteParticipation {
                    route_id,
                    private_journey_id,
                    joined_at: format_timestamp(joined_at)?,
                })
            })
            .collect()
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError> {
        if route_ids.is_empty() {
            return Ok(pb::RouteParticipationContext::default());
        }
        let counts = sqlx::query_as::<_, (String, i64)>(
            "SELECT route_id, SUM(active_count)::BIGINT FROM route_participation_count_shards WHERE route_id = ANY($1) GROUP BY route_id",
        )
        .bind(route_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let joined_route_ids = sqlx::query_scalar::<_, String>(
            "SELECT route_id FROM route_participations WHERE user_id = $1 AND route_id = ANY($2) AND left_at IS NULL ORDER BY route_id",
        )
        .bind(user_id)
        .bind(route_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(pb::RouteParticipationContext {
            joined_route_ids,
            participant_counts: counts
                .into_iter()
                .map(|(route_id, count)| (route_id, count.max(0) as u64))
                .collect(),
        })
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        // Only commands for the same user and route need ordering. Hot routes can use all shards.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::TEXT), hashtext($2::TEXT))")
            .bind(user_id)
            .bind(route_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;

        let intent_version =
            intent_version.map(|version| i64::try_from(version).unwrap_or(i64::MAX));

        if active {
            sqlx::query(
                "INSERT INTO route_participations (route_id, user_id, private_journey_id, left_at, last_intent_version) VALUES ($1, $2, $3, NULL, COALESCE($4, 0)) ON CONFLICT (route_id, user_id) DO UPDATE SET private_journey_id = EXCLUDED.private_journey_id, joined_at = CASE WHEN route_participations.left_at IS NULL THEN route_participations.joined_at ELSE now() END, left_at = NULL, last_intent_version = COALESCE($4, route_participations.last_intent_version) WHERE ($4 IS NOT NULL AND $4 >= route_participations.last_intent_version) OR ($4 IS NULL AND route_participations.last_intent_version = 0)",
            )
            .bind(route_id)
            .bind(user_id)
            .bind(private_journey_id)
            .bind(intent_version)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        } else {
            sqlx::query(
                "INSERT INTO route_participations (route_id, user_id, private_journey_id, left_at, last_intent_version) VALUES ($1, $2, NULL, now(), COALESCE($3, 0)) ON CONFLICT (route_id, user_id) DO UPDATE SET private_journey_id = NULL, left_at = CASE WHEN route_participations.left_at IS NULL THEN now() ELSE route_participations.left_at END, last_intent_version = COALESCE($3, route_participations.last_intent_version) WHERE ($3 IS NOT NULL AND $3 >= route_participations.last_intent_version) OR ($3 IS NULL AND route_participations.last_intent_version = 0)",
            )
            .bind(route_id)
            .bind(user_id)
            .bind(intent_version)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        let (private_journey_id, joined_at, left_at) = sqlx::query_as::<
            _,
            (
                Option<String>,
                time::OffsetDateTime,
                Option<time::OffsetDateTime>,
            ),
        >(
            "SELECT private_journey_id, joined_at, left_at FROM route_participations WHERE route_id = $1 AND user_id = $2",
        )
        .bind(route_id)
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let joined = left_at.is_none();
        let participant_count = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(active_count), 0)::BIGINT FROM route_participation_count_shards WHERE route_id = $1",
        )
        .bind(route_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(pb::RouteParticipationState {
            route_id: route_id.to_string(),
            joined,
            private_journey_id: joined.then_some(private_journey_id).flatten(),
            joined_at: if joined {
                Some(format_timestamp(joined_at)?)
            } else {
                None
            },
            participant_count: participant_count.max(0) as u64,
        })
    }
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
/// repository remains the source of truth for every relationship mutation and
/// for cache misses when Redis is unavailable.
pub(crate) struct CachedBbsRepository {
    inner: Arc<dyn BbsRepository>,
    redis: Option<ConnectionManager>,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

impl CachedBbsRepository {
    pub(crate) fn new(inner: Arc<dyn BbsRepository>, redis: Option<ConnectionManager>) -> Self {
        Self {
            inner,
            redis,
            miss_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn cached_message<M, F, Fut>(
        &self,
        kind: &str,
        user_id: &str,
        load: F,
    ) -> Result<M, RepositoryError>
    where
        M: Message + Default + Clone,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<M, RepositoryError>>,
    {
        let cache_key = relationship_cache_key(kind, user_id);
        if let Some(value) = self.load_message(kind, user_id).await {
            return Ok(value);
        }

        let _local = self.miss_lock(&cache_key).await;
        if let Some(value) = self.load_message(kind, user_id).await {
            return Ok(value);
        }

        let lease_key = relationship_refresh_key(kind, user_id);
        let lease = self.refresh_lock(&lease_key).await;
        if matches!(lease, RefreshLeaseDecision::Peer) {
            if let Some(value) = self.load_message(kind, user_id).await {
                return Ok(value);
            }
            return Err(RepositoryError::CachePeerRefresh);
        }

        let version_key = relationship_version_key(kind, user_id);
        let version = self.cache_version(&version_key).await;
        let result = load().await;
        if let (Some(version), Ok(value)) = (version, result.as_ref()) {
            self.store_message(&version_key, &cache_key, version, value, kind)
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
            tokio::time::Instant::now() + Duration::from_millis(RELATIONSHIP_REFRESH_LOCK_WAIT_MS);
        loop {
            let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
                .arg(key)
                .arg(&token)
                .arg("NX")
                .arg("PX")
                .arg(RELATIONSHIP_REFRESH_LOCK_TTL_MS)
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
                    sleep(Duration::from_millis(RELATIONSHIP_REFRESH_LOCK_POLL_MS)).await;
                }
                Ok(None) => {
                    tracing::debug!(
                        key,
                        "bbs relationship refresh lease held by another instance"
                    );
                    return RefreshLeaseDecision::Peer;
                }
                Err(error) => {
                    tracing::debug!(%error, key, "bbs relationship refresh lease unavailable; using repository");
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
                tracing::debug!(%error, key, "bbs relationship cache version read degraded");
                None
            }
        }
    }

    async fn load_message<M>(&self, kind: &str, user_id: &str) -> Option<M>
    where
        M: Message + Default,
    {
        let mut manager = self.redis.clone()?;
        let version_key = relationship_version_key(kind, user_id);
        let cache_key = relationship_cache_key(kind, user_id);
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
                match M::decode(&payload[std::mem::size_of::<u64>()..]) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        tracing::warn!(%error, kind, "bbs relationship cache payload invalid");
                        None
                    }
                }
            }
            Err(error) => {
                tracing::debug!(%error, kind, "bbs relationship cache read degraded");
                None
            }
        }
    }

    async fn store_message<M>(
        &self,
        version_key: &str,
        cache_key: &str,
        version: u64,
        value: &M,
        kind: &str,
    ) where
        M: Message,
    {
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
            .arg(RELATIONSHIP_CACHE_TTL_SECONDS)
            .arg(RELATIONSHIP_VERSION_TTL_SECONDS)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::debug!(%error, kind, "bbs relationship cache write degraded");
        }
    }

    async fn invalidate(&self, kind: &str, user_id: &str) {
        let Some(mut manager) = self.redis.clone() else {
            return;
        };
        let version_key = relationship_version_key(kind, user_id);
        let cache_key = relationship_cache_key(kind, user_id);
        let result: redis::RedisResult<i32> = redis::Script::new(INVALIDATE_CACHE)
            .key(version_key)
            .key(cache_key)
            .arg(RELATIONSHIP_VERSION_TTL_SECONDS)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, kind, "bbs relationship cache invalidation degraded");
        }
    }

    async fn invalidate_relationship(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
    ) {
        self.invalidate("context", user_id).await;
        self.invalidate("visibility", user_id).await;
        if edge == pb::SocialEdgeType::Block {
            self.invalidate("context", target_user_id).await;
            self.invalidate("visibility", target_user_id).await;
        }
    }
}

#[async_trait]
impl BbsRepository for CachedBbsRepository {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError> {
        self.cached_message("context", user_id, || self.inner.context(user_id))
            .await
    }

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError> {
        self.cached_message("visibility", user_id, || {
            self.inner.visibility_context(user_id)
        })
        .await
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError> {
        let context = self
            .inner
            .set_edge(user_id, target_user_id, edge, active)
            .await?;
        self.invalidate_relationship(user_id, target_user_id, edge)
            .await;
        Ok(context)
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError> {
        self.inner.list_route_participations(user_id).await
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError> {
        self.inner.route_context(user_id, route_ids).await
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError> {
        self.inner
            .set_route_participation(
                user_id,
                route_id,
                active,
                private_journey_id,
                intent_version,
            )
            .await
    }
}

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

    use super::{
        BbsRepository, CachedBbsRepository, MemoryBbsRepository, ordered_social_pair,
        relationship_cache_key,
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
        let first = relationship_cache_key("context", "user-a");
        let second = relationship_cache_key("context", "user-b");
        assert_ne!(first, second);
        assert!(!first.contains("user-a"));
        assert!(!second.contains("user-b"));
    }

    #[tokio::test]
    async fn relationship_cache_falls_back_to_the_repository_without_redis() {
        let repository = Arc::new(CachedBbsRepository::new(
            Arc::new(MemoryBbsRepository::seeded()),
            None,
        ));
        let context = repository.context("demo-user").await.expect("context");
        assert_eq!(context.followed_author_ids, vec!["author-changfeng"]);
    }
}
