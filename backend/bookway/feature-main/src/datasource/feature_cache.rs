use super::*;

#[derive(Clone)]
pub(crate) struct FeatureCache {
    redis: Option<redis::aio::ConnectionManager>,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

impl FeatureCache {
    pub(crate) fn new(redis: Option<redis::aio::ConnectionManager>) -> Self {
        Self {
            redis,
            miss_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub(crate) async fn miss_lock(&self, user_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self
                .miss_locks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(user_id).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                locks.insert(user_id.to_string(), Arc::downgrade(&lock));
                lock
            }
        };
        lock.lock_owned().await
    }

    pub(crate) async fn refresh_lock(&self, user_id: &str) -> FeatureRefreshGuard {
        let local = self.miss_lock(user_id).await;
        let Some(mut manager) = self.redis.clone() else {
            return FeatureRefreshGuard {
                _local: local,
                redis: None,
                peer_holds_lease: false,
            };
        };
        let key = format!("bookway:features:refresh-lock:{user_id}");
        let sequence = FEATURE_REFRESH_LOCK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let token = format!("{}-{timestamp}-{sequence}", std::process::id());
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(FEATURE_REFRESH_LOCK_WAIT_MS);
        loop {
            let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
                .arg(&key)
                .arg(&token)
                .arg("NX")
                .arg("PX")
                .arg(FEATURE_REFRESH_LOCK_TTL_MS)
                .query_async(&mut manager)
                .await;
            match result {
                Ok(Some(_)) => {
                    return FeatureRefreshGuard {
                        _local: local,
                        redis: Some(RedisRefreshLease {
                            manager,
                            key,
                            token,
                        }),
                        peer_holds_lease: false,
                    };
                }
                Ok(None) if tokio::time::Instant::now() < deadline => {
                    sleep(Duration::from_millis(FEATURE_REFRESH_LOCK_POLL_MS)).await;
                }
                Ok(None) => {
                    tracing::debug!(user_id, "feature refresh lease held by another instance");
                    return FeatureRefreshGuard {
                        _local: local,
                        redis: None,
                        peer_holds_lease: true,
                    };
                }
                Err(error) => {
                    tracing::debug!(%error, user_id, "feature refresh lease unavailable; using local lock");
                    return FeatureRefreshGuard {
                        _local: local,
                        redis: None,
                        peer_holds_lease: false,
                    };
                }
            }
        }
    }
    pub(crate) async fn load(&self, user_id: &str) -> Option<HashMap<String, f64>> {
        let mut manager = self.redis.clone()?;
        let result: Result<Option<String>, _> =
            redis::AsyncCommands::get(&mut manager, format!("bookway:features:{user_id}")).await;
        match result {
            Ok(Some(value)) => match serde_json::from_str(&value) {
                Ok(features) => Some(features),
                Err(error) => {
                    tracing::warn!(%error, user_id, "feature cache payload invalid");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, user_id, "feature cache read degraded");
                None
            }
        }
    }
    pub(crate) async fn store(&self, user_id: &str, features: &HashMap<String, f64>) {
        if let Some(mut manager) = self.redis.clone() {
            let value = serde_json::to_string(features).unwrap_or_else(|_| "{}".to_string());
            let result: Result<(), _> = redis::AsyncCommands::set_ex(
                &mut manager,
                format!("bookway:features:{user_id}"),
                value,
                60,
            )
            .await;
            if let Err(error) = result {
                tracing::warn!(%error, user_id, "feature cache write degraded");
            }
        }
    }
}
