use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::time::sleep;

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct CandidateFeatures {
    pub(crate) domain_affinity: f64,
    pub(crate) author_affinity: f64,
    pub(crate) impression_fatigue: f64,
    pub(crate) direct_negative_feedback: f64,
    pub(crate) click_through_rate: f64,
    pub(crate) save_rate: f64,
    pub(crate) action_completion_rate: f64,
    pub(crate) purchase_conversion_rate: f64,
    pub(crate) route_completion_rate: f64,
}

fn finite_features(features: serde_json::Value) -> HashMap<String, f64> {
    features
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(name, value)| {
            value
                .as_f64()
                .filter(|number| number.is_finite())
                .map(|number| (name.clone(), number))
        })
        .collect()
}

#[cfg(test)]
mod snapshot_tests {
    use super::finite_features;

    #[test]
    fn snapshot_payload_accepts_only_finite_numeric_features() {
        let features = finite_features(serde_json::json!({
            "domain_interest.learning": 0.8,
            "recent_positive_rate": 0.4,
            "label": "ignored",
            "nested": { "value": 1.0 }
        }));
        assert_eq!(features.get("domain_interest.learning"), Some(&0.8));
        assert_eq!(features.get("recent_positive_rate"), Some(&0.4));
        assert!(!features.contains_key("label"));
        assert!(!features.contains_key("nested"));
    }
}

const FEATURE_REFRESH_LOCK_TTL_MS: usize = 5_000;
const FEATURE_REFRESH_LOCK_WAIT_MS: u64 = 80;
const FEATURE_REFRESH_LOCK_POLL_MS: u64 = 10;
const FEATURE_REFRESH_LOCK_RELEASE: &str = r#"
if redis.call('get', KEYS[1]) == ARGV[1] then
  return redis.call('del', KEYS[1])
end
return 0
"#;
static FEATURE_REFRESH_LOCK_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct RedisRefreshLease {
    manager: redis::aio::ConnectionManager,
    key: String,
    token: String,
}

impl RedisRefreshLease {
    async fn release(self) {
        let mut manager = self.manager;
        let result: redis::RedisResult<i32> = redis::Script::new(FEATURE_REFRESH_LOCK_RELEASE)
            .key(self.key)
            .arg(self.token)
            .invoke_async(&mut manager)
            .await;
        if let Err(error) = result {
            tracing::debug!(%error, "feature refresh lease release degraded");
        }
    }
}

#[cfg(test)]
mod cache_tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use tokio::time::{Duration, sleep};

    use super::FeatureCache;

    #[tokio::test]
    async fn per_user_miss_lock_coalesces_concurrent_refreshes() {
        let cache = FeatureCache::new(None);
        let loads = Arc::new(AtomicUsize::new(0));
        let loaded = Arc::new(AtomicBool::new(false));
        let first = {
            let cache = cache.clone();
            let loads = loads.clone();
            let loaded = loaded.clone();
            tokio::spawn(async move {
                let _guard = cache.miss_lock("user-1").await;
                if !loaded.swap(true, Ordering::SeqCst) {
                    loads.fetch_add(1, Ordering::SeqCst);
                    sleep(Duration::from_millis(10)).await;
                }
            })
        };
        let second = {
            let cache = cache.clone();
            let loads = loads.clone();
            let loaded = loaded.clone();
            tokio::spawn(async move {
                let _guard = cache.miss_lock("user-1").await;
                if !loaded.swap(true, Ordering::SeqCst) {
                    loads.fetch_add(1, Ordering::SeqCst);
                }
            })
        };
        first.await.expect("first refresh task should finish");
        second.await.expect("second refresh task should finish");
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn refresh_lock_falls_back_to_the_local_guard_without_redis() {
        let cache = FeatureCache::new(None);
        let guard = cache.refresh_lock("user-1").await;
        guard.release().await;
    }
}

#[path = "feature_dao.rs"]
mod feature_dao;
pub(crate) use feature_dao::FeatureDao;
#[path = "feature_cache.rs"]
mod feature_cache;
pub(crate) use feature_cache::FeatureCache;
#[path = "feature_refresh_guard.rs"]
mod feature_refresh_guard;
pub(crate) use feature_refresh_guard::FeatureRefreshGuard;
