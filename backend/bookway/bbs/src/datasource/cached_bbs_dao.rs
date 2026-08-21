use super::*;

pub(crate) struct CachedBbsDao {
    inner: Arc<dyn BbsDao>,
    redis: Option<ConnectionManager>,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

impl CachedBbsDao {
    pub(crate) fn new(inner: Arc<dyn BbsDao>, redis: Option<ConnectionManager>) -> Self {
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
    ) -> Result<M, DaoError>
    where
        M: Message + Default + Clone,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<M, DaoError>>,
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
            return Err(DaoError::CachePeerRefresh);
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
                    tracing::debug!(%error, key, "bbs relationship refresh lease unavailable; using Dao");
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
impl BbsDao for CachedBbsDao {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, DaoError> {
        self.cached_message("context", user_id, || self.inner.context(user_id))
            .await
    }

    async fn visibility_context(&self, user_id: &str) -> Result<pb::SocialVisibility, DaoError> {
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
    ) -> Result<pb::SocialContext, DaoError> {
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
    ) -> Result<Vec<pb::RouteParticipation>, DaoError> {
        self.inner.list_route_participations(user_id).await
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, DaoError> {
        self.inner.route_context(user_id, route_ids).await
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, DaoError> {
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
