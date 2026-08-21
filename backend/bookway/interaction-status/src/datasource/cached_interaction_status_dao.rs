use super::*;

pub(crate) struct CachedInteractionStatusDao {
    inner: Arc<dyn InteractionStatusDao>,
    redis: Option<ConnectionManager>,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

impl CachedInteractionStatusDao {
    pub(crate) fn new(
        inner: Arc<dyn InteractionStatusDao>,
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
    ) -> Result<pb::ReactionContext, DaoError> {
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
            return Err(DaoError::CachePeerRefresh);
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
                    tracing::debug!(%error, key, "interaction context refresh lease unavailable; using Dao");
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
impl InteractionStatusDao for CachedInteractionStatusDao {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, DaoError> {
        self.cached_context(user_id, post_ids).await
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, DaoError> {
        let result = self
            .inner
            .set_reaction(user_id, post_id, reaction, active)
            .await?;
        self.invalidate(user_id).await;
        Ok(result)
    }
}
