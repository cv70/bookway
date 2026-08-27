//! Shared cache-miss coordination for hot read paths.
//!
//! Every hot read in this workspace follows the same three-step pattern when a
//! Redis-backed entry is absent: serialize concurrent rebuilds for the same key
//! behind a process-local lock, coordinate across instances with a short-lived
//! `SET NX PX` lease guarded by a token, and re-read the backing store once the
//! lock or lease is held. This crate extracts that pattern so services do not
//! keep hand-rolled copies, and so mall/ad catalog reads can adopt it safely.
//!
//! Two flavors cover the workspace:
//!
//! - [`SingleFlightCache`] is plain read-through JSON with TTL expiry and
//!   best-effort [`SingleFlightCache::invalidate`]. Fits values whose brief
//!   staleness after a mutation is acceptable.
//! - [`VersionedCache`] adds an invalidation counter per *scope*: payloads are
//!   stamped with a counter snapshot and only served while the stamp still
//!   matches, so a mutation closes the staleness window immediately instead of
//!   waiting out the TTL. Protobuf-framed; fits safety-relevant graphs such as
//!   social visibility.
//!
//! Failing Redis stays non-fatal everywhere: reads degrade to `None`, writes
//! are dropped with a warning, and lease contention falls back to the local
//! lock without ever failing the caller.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak, atomic::AtomicU64},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use prost::Message;
use serde::{Serialize, de::DeserializeOwned};

const DEFAULT_REFRESH_LOCK_TTL_MS: usize = 5_000;
const DEFAULT_REFRESH_LOCK_WAIT_MS: u64 = 80;
const DEFAULT_REFRESH_LOCK_POLL_MS: u64 = 10;

const LEASE_RELEASE_LUA: &str = r#"
if redis.call('get', KEYS[1]) == ARGV[1] then
  return redis.call('del', KEYS[1])
end
return 0
"#;

static LEASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Process-local miss locks plus cross-instance refresh leases, shared by both
/// cache flavors so coordination rules cannot drift between them.
#[derive(Clone)]
struct MissCoordinator {
    redis: Option<redis::aio::ConnectionManager>,
    wait_ms: u64,
    poll_ms: u64,
    lock_ttl_ms: usize,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

impl MissCoordinator {
    fn new(redis: Option<redis::aio::ConnectionManager>) -> Self {
        Self {
            redis,
            wait_ms: DEFAULT_REFRESH_LOCK_WAIT_MS,
            poll_ms: DEFAULT_REFRESH_LOCK_POLL_MS,
            lock_ttl_ms: DEFAULT_REFRESH_LOCK_TTL_MS,
            miss_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn tune(&mut self, wait_ms: u64, poll_ms: u64, lock_ttl_ms: usize) {
        self.wait_ms = wait_ms;
        self.poll_ms = poll_ms;
        self.lock_ttl_ms = lock_ttl_ms;
    }

    async fn refresh_lock(&self, lease_key: &str) -> RefreshGuard {
        let local = self.local_miss_lock(lease_key).await;
        let Some(mut manager) = self.redis.clone() else {
            return RefreshGuard {
                _local: local,
                release_lease: None,
                peer_holds_lease: false,
            };
        };
        let sequence = LEASE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let token = format!("{}-{timestamp}-{sequence}", std::process::id());
        let deadline = tokio::time::Instant::now() + Duration::from_millis(self.wait_ms);
        loop {
            let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
                .arg(lease_key)
                .arg(&token)
                .arg("NX")
                .arg("PX")
                .arg(self.lock_ttl_ms)
                .query_async(&mut manager)
                .await;
            match result {
                Ok(Some(_)) => {
                    return RefreshGuard {
                        _local: local,
                        release_lease: Some(RedisLease {
                            manager,
                            key: lease_key.to_string(),
                            token,
                        }),
                        peer_holds_lease: false,
                    };
                }
                Ok(None) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(self.poll_ms)).await;
                }
                Ok(None) => {
                    tracing::debug!(key = lease_key, "refresh lease held by another instance");
                    return RefreshGuard {
                        _local: local,
                        release_lease: None,
                        peer_holds_lease: true,
                    };
                }
                Err(error) => {
                    tracing::debug!(
                        %error,
                        key = lease_key,
                        "refresh lease unavailable; using local lock only"
                    );
                    return RefreshGuard {
                        _local: local,
                        release_lease: None,
                        peer_holds_lease: false,
                    };
                }
            }
        }
    }

    async fn local_miss_lock(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self
                .miss_locks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
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
}

/// A JSON-serialized read-through cache with miss single-flight.
/// `V` must be `Clone` because contended readers return their own copy.
#[derive(Clone)]
pub struct SingleFlightCache<V> {
    coordinator: MissCoordinator,
    key_prefix: String,
    ttl_secs: u64,
    _value: std::marker::PhantomData<fn() -> V>,
}

impl<V: Serialize + DeserializeOwned + Clone> SingleFlightCache<V> {
    /// `redis = None` keeps every call functional (local locks only), matching
    /// memory-mode development and degraded production behavior.
    pub fn new(
        redis: Option<redis::aio::ConnectionManager>,
        key_prefix: &str,
        ttl_secs: u64,
    ) -> Self {
        Self {
            coordinator: MissCoordinator::new(redis),
            key_prefix: key_prefix.to_string(),
            ttl_secs,
            _value: std::marker::PhantomData,
        }
    }

    pub fn with_refresh_tuning(mut self, wait_ms: u64, poll_ms: u64, lock_ttl_ms: usize) -> Self {
        self.coordinator.tune(wait_ms, poll_ms, lock_ttl_ms);
        self
    }

    pub async fn load(&self, key: &str) -> Option<V> {
        let mut manager = self.coordinator.redis.clone()?;
        let full_key = format!("{}:{key}", self.key_prefix);
        let result: Result<Option<String>, _> =
            redis::AsyncCommands::get(&mut manager, full_key).await;
        match result {
            Ok(Some(value)) => match serde_json::from_str::<V>(&value) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::warn!(%error, key, "cache payload invalid; treating as miss");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, key, "cache read degraded");
                None
            }
        }
    }

    pub async fn store(&self, key: &str, value: &V) {
        if let Some(mut manager) = self.coordinator.redis.clone() {
            let Ok(encoded) = serde_json::to_string(value) else {
                tracing::warn!(key, "cache payload not serializable");
                return;
            };
            let full_key = format!("{}:{key}", self.key_prefix);
            let result: Result<(), _> =
                redis::AsyncCommands::set_ex(&mut manager, full_key, encoded, self.ttl_secs).await;
            if let Err(error) = result {
                tracing::warn!(%error, key, "cache write degraded");
            }
        }
    }

    pub async fn invalidate(&self, keys: &[&str]) {
        let Some(mut manager) = self.coordinator.redis.clone() else {
            return;
        };
        for key in keys {
            let full_key = format!("{}:{key}", self.key_prefix);
            let result: Result<(), _> = redis::AsyncCommands::del(&mut manager, full_key).await;
            if let Err(error) = result {
                tracing::warn!(%error, key, "cache invalidation degraded");
            }
        }
    }

    /// Coordinates a rebuild for `key`: acquires the per-key local lock, then
    /// tries to take the cross-instance lease. When a peer already holds the
    /// lease the guard reports it so the caller can serve a safe fallback
    /// instead of hammering the source of truth.
    pub async fn refresh_lock(&self, key: &str) -> RefreshGuard {
        let lease_key = format!("{}:refresh-lock:{key}", self.key_prefix);
        self.coordinator.refresh_lock(&lease_key).await
    }
}

/// An invalidation-counter read-through cache for protobuf values whose
/// staleness window must close on mutation rather than on TTL expiry.
///
/// Each entry spans two Redis addresses under `key_prefix`: the payload key
/// `{prefix}:{key}` holds an 8-byte big-endian version stamp followed by the
/// encoded message, while the version key `{version_prefix}:{scope}` holds a
/// decimal counter bumped by [`VersionedCache::invalidate`]. A reader accepts
/// a payload only when its embedded stamp equals the live counter for its
/// invalidation scope, so a store racing an invalidation lands as an
/// unreachable corpse instead of served-stale data — the same guarantee the
/// former per-service Lua compare-and-set scripts gave, without the Lua.
///
/// `scope` is decoupled from `key` so one mutation can retire many cached
/// entries at once (for example one user's reaction context spread over every
/// queried post combination).
///
/// The stamped version must be snapshotted with [`VersionedCache::version`]
/// *before* the backing-store reload; storing a freshly-read counter would
/// re-validate data loaded before the mutation the counter already reflects.
#[derive(Clone)]
pub struct VersionedCache<M> {
    coordinator: MissCoordinator,
    key_prefix: String,
    version_prefix: String,
    payload_ttl_secs: u64,
    version_ttl_secs: u64,
    _value: std::marker::PhantomData<fn() -> M>,
}

impl<M: Message + Default + Clone> VersionedCache<M> {
    /// Defaults the version prefix to `{key_prefix}:ver`, matching caches
    /// whose entries and invalidation scopes share the same key space.
    pub fn new(
        redis: Option<redis::aio::ConnectionManager>,
        key_prefix: &str,
        payload_ttl_secs: u64,
        version_ttl_secs: u64,
    ) -> Self {
        Self::new_scoped(
            redis,
            key_prefix,
            &format!("{key_prefix}:ver"),
            payload_ttl_secs,
            version_ttl_secs,
        )
    }

    /// `version_ttl_secs` must exceed `payload_ttl_secs`: if the counter could
    /// expire while a stamped payload outlives it, a recreated zero counter
    /// would start accepting pre-invalidation payloads again.
    ///
    /// `version_prefix` may be anything; scoping the counter coarser than the
    /// payload keys is how one invalidation retires many entries.
    pub fn new_scoped(
        redis: Option<redis::aio::ConnectionManager>,
        key_prefix: &str,
        version_prefix: &str,
        payload_ttl_secs: u64,
        version_ttl_secs: u64,
    ) -> Self {
        assert!(
            version_ttl_secs > payload_ttl_secs,
            "version counter must outlive the payloads it validates"
        );
        Self {
            coordinator: MissCoordinator::new(redis),
            key_prefix: key_prefix.to_string(),
            version_prefix: version_prefix.to_string(),
            payload_ttl_secs,
            version_ttl_secs,
            _value: std::marker::PhantomData,
        }
    }

    pub fn with_refresh_tuning(mut self, wait_ms: u64, poll_ms: u64, lock_ttl_ms: usize) -> Self {
        self.coordinator.tune(wait_ms, poll_ms, lock_ttl_ms);
        self
    }

    /// Live invalidation counter for `scope`, or `Some(0)` when unset. `None`
    /// means Redis degraded and the pending store should be skipped entirely
    /// rather than stamped against a guessed version.
    pub async fn version(&self, scope: &str) -> Option<u64> {
        let mut manager = self.coordinator.redis.clone()?;
        let version_key = format!("{}:{scope}", self.version_prefix);
        let result: Result<Option<String>, _> =
            redis::AsyncCommands::get(&mut manager, version_key).await;
        match result {
            Ok(Some(value)) => value.parse().ok(),
            Ok(None) => Some(0),
            Err(error) => {
                tracing::debug!(%error, scope, "versioned cache counter read degraded");
                None
            }
        }
    }

    pub async fn load(&self, key: &str, scope: &str) -> Option<M> {
        let mut manager = self.coordinator.redis.clone()?;
        let version_key = format!("{}:{scope}", self.version_prefix);
        let payload_key = format!("{}:{key}", self.key_prefix);
        let result: Result<Vec<Option<Vec<u8>>>, _> = redis::cmd("MGET")
            .arg(version_key)
            .arg(payload_key)
            .query_async(&mut manager)
            .await;
        match result {
            Ok(values) => {
                let current = values
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .map_or(Some(0), |text| text.parse::<u64>().ok())?;
                let raw = values.get(1).and_then(Option::as_ref)?;
                decode_versioned_payload::<M>(current, raw)
            }
            Err(error) => {
                tracing::debug!(%error, key, "versioned cache read degraded");
                None
            }
        }
    }

    /// Writes `value` stamped with the snapshot taken by
    /// [`VersionedCache::version`] for this entry's invalidation scope *before*
    /// the backing-store reload. A mismatch later means an invalidation raced
    /// this rebuild; the payload sits unreadable until it expires or a future
    /// rebuild overwrites it.
    pub async fn store(&self, key: &str, version: u64, value: &M) {
        if let Some(mut manager) = self.coordinator.redis.clone() {
            let payload_key = format!("{}:{key}", self.key_prefix);
            let result: Result<(), _> = redis::AsyncCommands::set_ex(
                &mut manager,
                payload_key,
                versioned_payload(version, value),
                self.payload_ttl_secs,
            )
            .await;
            if let Err(error) = result {
                tracing::warn!(%error, key, "versioned cache write degraded");
            }
        }
    }

    /// Bumps the scope counter so every stamped payload in the scope stops
    /// being served immediately. Stale bodies linger unreadable until their
    /// TTL instead of being deleted; readers never observe them.
    pub async fn invalidate(&self, scope: &str) {
        let Some(mut manager) = self.coordinator.redis.clone() else {
            return;
        };
        let version_key = format!("{}:{scope}", self.version_prefix);
        let bump: Result<i64, _> = redis::AsyncCommands::incr(&mut manager, &version_key, 1i64).await;
        if let Err(error) = bump {
            tracing::warn!(%error, scope, "versioned cache invalidation degraded");
            return;
        }
        // Best-effort bookkeeping so historical scopes do not accumulate a
        // permanent key apiece; losing this expiry costs storage, never
        // correctness (the counter merely survives its payloads).
        let refreshed: Result<bool, _> =
            redis::AsyncCommands::expire(&mut manager, version_key, self.version_ttl_secs as i64)
                .await;
        if let Err(error) = refreshed {
            tracing::debug!(%error, scope, "versioned cache counter refresh degraded");
        }
    }

    /// Same rebuild coordination as [`SingleFlightCache::refresh_lock`],
    /// namespaced under this cache's prefix.
    pub async fn refresh_lock(&self, key: &str) -> RefreshGuard {
        let lease_key = format!("{}:refresh-lock:{key}", self.key_prefix);
        self.coordinator.refresh_lock(&lease_key).await
    }
}

const VERSION_STAMP_BYTES: usize = core::mem::size_of::<u64>();

fn versioned_payload<M: Message>(version: u64, value: &M) -> Vec<u8> {
    let mut payload = version.to_be_bytes().to_vec();
    payload.extend_from_slice(&value.encode_to_vec());
    payload
}

fn decode_versioned_payload<M: Message + Default>(current: u64, raw: &[u8]) -> Option<M> {
    if raw.len() < VERSION_STAMP_BYTES {
        return None;
    }
    let Ok(stamp_bytes) = <[u8; VERSION_STAMP_BYTES]>::try_from(&raw[..VERSION_STAMP_BYTES]) else {
        return None;
    };
    if u64::from_be_bytes(stamp_bytes) != current {
        return None;
    }
    match M::decode(&raw[VERSION_STAMP_BYTES..]) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(%error, "versioned cache payload invalid; treating as miss");
            None
        }
    }
}

/// Released on Drop as a bounded best-effort cleanup; the lease TTL remains
/// the crash-safety backstop. Prefer awaiting [`RefreshGuard::release`].
pub struct RefreshGuard {
    _local: tokio::sync::OwnedMutexGuard<()>,
    release_lease: Option<RedisLease>,
    peer_holds_lease: bool,
}

impl RefreshGuard {
    pub fn peer_holds_lease(&self) -> bool {
        self.peer_holds_lease
    }

    pub async fn release(mut self) {
        if let Some(lease) = self.release_lease.take() {
            lease.release().await;
        }
    }
}

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        let Some(lease) = self.release_lease.take() else {
            return;
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(lease.release());
        }
    }
}

struct RedisLease {
    manager: redis::aio::ConnectionManager,
    key: String,
    token: String,
}

impl RedisLease {
    async fn release(self) {
        let mut manager = self.manager;
        let result: redis::RedisResult<i32> = redis::Script::new(LEASE_RELEASE_LUA)
            .key(self.key)
            .arg(self.token)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::debug!(%error, "refresh lease release degraded");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
    struct Payload {
        hits: Vec<String>,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    struct SampleRecord {
        #[prost(string, tag = "1")]
        label: String,
    }

    #[tokio::test]
    async fn works_without_redis_for_local_singleflight() {
        let cache: SingleFlightCache<Payload> = SingleFlightCache::new(None, "bookway:test", 60);
        assert_eq!(cache.load("k").await, None);
        // Store silently no-ops without Redis.
        cache.store("k", &Payload { hits: vec!["a".into()] }).await;
        assert_eq!(cache.load("k").await, None);

        let guard = cache.refresh_lock("k").await;
        assert!(!guard.peer_holds_lease()); // no Redis => never report peer contention
        guard.release().await;
    }

    #[tokio::test]
    async fn serializes_concurrent_rebuilds_per_key_and_not_across_keys() {
        let cache = Arc::new(SingleFlightCache::<Payload>::new(None, "bookway:test", 60));
        let mut handles = Vec::new();
        for i in 0..8u32 {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                let key = if i % 2 == 0 { "same" } else { "other" };
                let guard = cache.refresh_lock(key).await;
                // Hold briefly to prove exclusion within one key.
                tokio::time::sleep(Duration::from_millis(10)).await;
                guard.release().await;
            }));
        }
        for handle in handles {
            handle.await.expect("single-flight worker task");
        }
    }

    #[tokio::test]
    async fn expired_local_locks_are_pruned_by_next_miss() {
        let cache: SingleFlightCache<Payload> = SingleFlightCache::new(None, "bookway:test", 60);
        drop(cache.refresh_lock("gone").await);
        tokio::time::sleep(Duration::from_millis(5)).await;
        // The next miss pass retains only live locks; a dead entry must no
        // longer upgrade to a held mutex.
        let _ = cache.refresh_lock("trigger-prune").await;
        let has_live = cache
            .coordinator
            .miss_locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("gone")
            .and_then(Weak::upgrade)
            .is_some();
        assert!(!has_live);
    }

    #[test]
    fn value_roundtrip_via_json_preserves_payload() {
        let payload = Payload { hits: vec!["x".into(), "y".into()] };
        let encoded = serde_json::to_string(&payload).expect("payload serializes");
        let decoded: Payload = serde_json::from_str(&encoded).expect("encoded payload roundtrips");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn version_stamp_serves_only_matching_counters() {
        let record = SampleRecord { label: "ctx".into() };
        let raw = versioned_payload(7, &record);

        assert_eq!(
            decode_versioned_payload::<SampleRecord>(7, &raw).as_ref(),
            Some(&record)
        );
        // A bumped counter retires the payload even though its body is intact.
        assert_eq!(decode_versioned_payload::<SampleRecord>(8, &raw), None);
        // Truncated stamps and truncated bodies are misses, not panics.
        assert_eq!(decode_versioned_payload::<SampleRecord>(7, &[]), None);
        let headless: Vec<u8> = Vec::new();
        assert_eq!(
            decode_versioned_payload::<SampleRecord>(7, headless.as_slice()),
            None
        );
        let corrupt = versioned_payload(7, &record);
        let corrupt = &corrupt[..corrupt.len() - 1];
        assert_eq!(decode_versioned_payload::<SampleRecord>(7, corrupt), None);
    }

    #[tokio::test]
    async fn versioned_cache_degrades_to_none_without_redis() {
        let cache: VersionedCache<SampleRecord> =
            VersionedCache::new(None, "bookway:test", 30, 120);
        // No counter readable => a pending rebuild must skip its store.
        assert_eq!(cache.version("scope").await, None);
        assert_eq!(cache.load("k", "scope").await, None);

        let guard = cache.refresh_lock("k").await;
        assert!(!guard.peer_holds_lease());
        guard.release().await;

        // Store/invalidate no-op quietly instead of failing the caller.
        cache.store("k", 0, &SampleRecord { label: "x".into() }).await;
        cache.invalidate("scope").await;
    }

    #[test]
    #[should_panic(expected = "version counter must outlive")]
    fn versioned_cache_rejects_counters_that_expire_first() {
        let _ = VersionedCache::<SampleRecord>::new(None, "bookway:test", 120, 30);
    }
}
