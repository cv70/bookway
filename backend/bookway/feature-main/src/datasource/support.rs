use std::collections::HashMap;

use serde::Serialize;

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

/// Per-user feature read-through cache. The miss single-flight (local lock +
/// cross-instance `SET NX PX` lease, Redis-fail-open) now lives in the shared
/// `bookway_cache` crate; keys stay `bookway:features:{user_id}` with a 60s
/// TTL so existing deployments keep the same cache layout.
pub(crate) type UserFeatureCache = bookway_cache::SingleFlightCache<HashMap<String, f64>>;

pub(crate) fn user_feature_cache(redis: Option<redis::aio::ConnectionManager>) -> UserFeatureCache {
    bookway_cache::SingleFlightCache::new(redis, "bookway:features", 60)
        .with_refresh_tuning(
            FEATURE_REFRESH_LOCK_WAIT_MS,
            FEATURE_REFRESH_LOCK_POLL_MS,
            FEATURE_REFRESH_LOCK_TTL_MS,
        )
}

#[cfg(test)]
mod cache_tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use tokio::time::{Duration, sleep};

    use super::{user_feature_cache, UserFeatureCache};

    #[tokio::test]
    async fn per_user_miss_lock_coalesces_concurrent_refreshes() {
        let cache: UserFeatureCache = user_feature_cache(None);
        let loads = Arc::new(AtomicUsize::new(0));
        let loaded = Arc::new(AtomicBool::new(false));
        let first = {
            let cache = cache.clone();
            let loads = loads.clone();
            let loaded = loaded.clone();
            tokio::spawn(async move {
                let _guard = cache.refresh_lock("user-1").await;
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
                let _guard = cache.refresh_lock("user-1").await;
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
        let cache: UserFeatureCache = user_feature_cache(None);
        let guard = cache.refresh_lock("user-1").await;
        guard.release().await;
    }
}

#[path = "feature_dao.rs"]
mod feature_dao;
pub(crate) use feature_dao::FeatureDao;
