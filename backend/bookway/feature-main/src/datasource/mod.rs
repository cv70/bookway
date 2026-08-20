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
}

#[derive(Clone)]
pub(crate) struct FeatureRepository {
    pool: Option<sqlx::PgPool>,
    feature_version: String,
}
impl FeatureRepository {
    pub(crate) fn new(pool: Option<sqlx::PgPool>, feature_version: String) -> Self {
        Self {
            pool,
            feature_version,
        }
    }
    pub(crate) async fn load(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        let mut derived = self.load_snapshot(user_id).await;
        // Keep feature freshness bounded while deriving feedback features
        // from the canonical event log. The event types are intentionally
        // weighted so a repeat dismissal does not look like topic rejection,
        // while relevance and safety feedback can meaningfully reduce exploration.
        let feedback = sqlx::query_as::<_, (i64, f64, i64, i64)>(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE event_type IN ('like', 'bookmark', 'save_knowledge', 'share', 'complete')),
                COALESCE(SUM(CASE
                    WHEN event_type = 'hide' AND negative_feedback_reason = 'already_seen' THEN 0.25
                    WHEN event_type IN ('hide', 'report') THEN 1.0
                    ELSE 0.0
                END), 0)::double precision,
                COUNT(*) FILTER (WHERE event_type IN ('impression', 'view')),
                COUNT(*)
            FROM user_events
            WHERE user_id = $1 AND occurred_at > now() - interval '30 days'
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or((0, 0.0, 0, 0));
        let positive = feedback.0 as f64;
        let negative = feedback.1;
        let impressions = feedback.2 as f64;
        let total = feedback.3 as f64;
        derived.extend([
            (
                "recent_positive_rate".to_string(),
                (positive / impressions.max(1.0)).min(1.0),
            ),
            (
                "negative_feedback_rate".to_string(),
                (negative / impressions.max(1.0)).min(1.0),
            ),
            (
                "user_interest_strength".to_string(),
                ((positive - negative * 0.75) / total.max(1.0)).clamp(0.0, 1.0),
            ),
        ]);
        derived.extend(self.load_domain_interests(user_id).await);
        let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT feature_name,value FROM user_features WHERE user_id=$1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await;
        match rows {
            Ok(rows) => {
                derived.extend(
                    rows.into_iter()
                        .filter_map(|(name, value)| value.as_f64().map(|number| (name, number))),
                );
                derived
            }
            Err(error) => {
                tracing::warn!(%error, user_id, "feature store degraded");
                derived
            }
        }
    }

    async fn load_snapshot(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        let row = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT features FROM user_feature_snapshots WHERE user_id=$1 AND feature_version=$2 AND expires_at > now() ORDER BY as_of DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(&self.feature_version)
        .fetch_optional(pool)
        .await;
        match row {
            Ok(Some((features,))) => finite_features(features),
            Ok(None) => HashMap::new(),
            Err(error) => {
                tracing::warn!(%error, user_id, version = %self.feature_version, "feature snapshot read degraded");
                HashMap::new()
            }
        }
    }

    async fn load_domain_interests(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        // These are user-level features, used before candidate generation so
        // a strong interest can expand recall instead of only reranking it.
        let rows = sqlx::query_as::<_, (String, f64)>(
            r#"
            SELECT
                content.domain,
                SUM(
                    CASE event.event_type
                        WHEN 'join_route' THEN 5.0
                        WHEN 'complete' THEN 5.0
                        WHEN 'save_knowledge' THEN 4.0
                        WHEN 'bookmark' THEN 3.0
                        WHEN 'share' THEN 2.5
                        WHEN 'like' THEN 2.0
                        WHEN 'click' THEN 0.6
                        WHEN 'view' THEN 0.4
                        WHEN 'hide' THEN CASE
                            WHEN event.negative_feedback_reason IN ('already_seen', 'low_quality') THEN 0.0
                            ELSE -5.0
                        END
                        WHEN 'report' THEN -8.0
                        ELSE 0.0
                    END
                )::double precision
            FROM user_events AS event
            INNER JOIN content_items AS content ON content.id = event.content_id
            WHERE event.user_id = $1
              AND event.occurred_at > now() - interval '90 days'
            GROUP BY content.domain
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, user_id, "domain interest features degraded");
                return HashMap::new();
            }
        };
        let maximum = rows
            .iter()
            .map(|(_, score)| score.max(0.0))
            .fold(1.0_f64, f64::max);
        rows.into_iter()
            .filter(|(domain, _)| {
                matches!(
                    domain.as_str(),
                    "learning" | "movement" | "wellness" | "travel" | "leisure"
                )
            })
            .filter_map(|(domain, score)| {
                let score = (score.max(0.0) / maximum).clamp(0.0, 1.0);
                (score > 0.0).then(|| (format!("domain_interest.{domain}"), score))
            })
            .collect()
    }

    pub(crate) async fn load_candidates(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> HashMap<String, CandidateFeatures> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        if content_ids.is_empty() {
            return HashMap::new();
        }

        // Normalize high-intent history within each user's strongest domain
        // and author so global popularity does not erase personal preference.
        let rows = sqlx::query_as::<_, (String, f64, f64, f64, f64, f64, f64, f64, f64)>(
            r#"
            WITH history AS (
                SELECT
                    content.domain,
                    content.author_id,
                    CASE event.event_type
                        WHEN 'join_route' THEN 5.0
                        WHEN 'complete' THEN 5.0
                        WHEN 'save_knowledge' THEN 4.0
                        WHEN 'bookmark' THEN 3.0
                        WHEN 'share' THEN 2.5
                        WHEN 'like' THEN 2.0
                        WHEN 'click' THEN 0.6
                        WHEN 'view' THEN 0.4
                        WHEN 'hide' THEN CASE
                            WHEN event.negative_feedback_reason IN ('already_seen', 'low_quality') THEN 0.0
                            ELSE -5.0
                        END
                        WHEN 'report' THEN -8.0
                        ELSE 0.0
                    END AS domain_weight,
                    CASE event.event_type
                        WHEN 'join_route' THEN 5.0
                        WHEN 'complete' THEN 5.0
                        WHEN 'save_knowledge' THEN 4.0
                        WHEN 'bookmark' THEN 3.0
                        WHEN 'share' THEN 2.5
                        WHEN 'like' THEN 2.0
                        WHEN 'click' THEN 0.6
                        WHEN 'view' THEN 0.4
                        WHEN 'hide' THEN CASE
                            WHEN event.negative_feedback_reason = 'already_seen' THEN 0.0
                            WHEN event.negative_feedback_reason = 'not_relevant' THEN 0.0
                            WHEN event.negative_feedback_reason = 'low_quality' THEN -4.0
                            ELSE -5.0
                        END
                        WHEN 'report' THEN -8.0
                        ELSE 0.0
                    END AS author_weight
                FROM user_events AS event
                INNER JOIN content_items AS content ON content.id = event.content_id
                WHERE event.user_id = $1
                  AND event.occurred_at > now() - interval '90 days'
            ),
            domain_scores AS (
                SELECT domain, SUM(domain_weight)::double precision AS score
                FROM history
                GROUP BY domain
            ),
            author_scores AS (
                SELECT author_id, SUM(author_weight)::double precision AS score
                FROM history
                GROUP BY author_id
            ),
            normalizers AS (
                SELECT
                    GREATEST(COALESCE((SELECT MAX(score) FROM domain_scores), 0.0), 1.0) AS domain_max,
                    GREATEST(COALESCE((SELECT MAX(score) FROM author_scores), 0.0), 1.0) AS author_max
            ),
            direct_feedback AS (
                SELECT
                    content_id,
                    COUNT(*) FILTER (
                        WHERE event_type = 'impression'
                          AND occurred_at > now() - interval '30 days'
                    )::double precision AS impression_count,
                    SUM(CASE
                        WHEN event_type = 'hide'
                             AND negative_feedback_reason = 'already_seen'
                             AND occurred_at > now() - interval '90 days' THEN 0.25
                        WHEN event_type IN ('hide', 'report')
                             AND occurred_at > now() - interval '90 days' THEN 1.0
                        ELSE 0.0
                    END)::double precision AS negative_weight
                    ,COUNT(*) FILTER (
                        WHERE event_type = 'click'
                          AND occurred_at > now() - interval '30 days'
                    )::double precision AS clicks
                    ,COUNT(*) FILTER (WHERE event_type IN ('bookmark', 'save_knowledge'))::double precision AS saves
                    ,COUNT(*) FILTER (WHERE event_type = 'save_knowledge')::double precision AS knowledge_starts
                    ,COUNT(*) FILTER (WHERE event_type = 'complete')::double precision AS completions
                    ,COUNT(*) FILTER (WHERE event_type = 'join_route')::double precision AS joins
                    ,COUNT(*) FILTER (WHERE event_type = 'purchase')::double precision AS purchases
                FROM user_events
                WHERE user_id = $1
                  AND content_id = ANY($2)
                  AND occurred_at > now() - interval '90 days'
                GROUP BY content_id
            ),
            population_feedback AS (
                -- Population signals provide a cold-start prior for pCTR,
                -- pCVR and route completion. Personal signals take over only
                -- after enough observations to avoid one-event overfitting.
                SELECT
                    content_id,
                    COUNT(*) FILTER (
                        WHERE event_type = 'impression'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS impression_count,
                    COUNT(*) FILTER (
                        WHERE event_type = 'click'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS clicks,
                    COUNT(*) FILTER (
                        WHERE event_type = 'complete'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS completions,
                    COUNT(*) FILTER (
                        WHERE event_type = 'save_knowledge'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS knowledge_starts,
                    COUNT(*) FILTER (
                        WHERE event_type = 'join_route'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS joins,
                    COUNT(*) FILTER (
                        WHERE event_type = 'purchase'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS purchases
                FROM user_events
                WHERE content_id = ANY($2)
                  AND occurred_at > now() - interval '90 days'
                GROUP BY content_id
            )
            SELECT
                candidate.id,
                LEAST(GREATEST(COALESCE(domain.score, 0.0), 0.0) / normalizers.domain_max, 1.0)::double precision,
                LEAST(GREATEST(COALESCE(author.score, 0.0), 0.0) / normalizers.author_max, 1.0)::double precision,
                LEAST(COALESCE(feedback.impression_count, 0.0) / 4.0, 1.0)::double precision,
                LEAST(COALESCE(feedback.negative_weight, 0.0), 1.0)::double precision,
                LEAST(
                    COALESCE(feedback.clicks, 0.0)
                        / GREATEST(COALESCE(feedback.impression_count, 0.0), 1.0)
                        * LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0)
                    + (COALESCE(population.clicks, 0.0) + 0.5)
                        / GREATEST(COALESCE(population.impression_count, 0.0) + 20.0, 20.0)
                        * (1.0 - LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0)),
                    1.0
                )::double precision,
                LEAST(COALESCE(feedback.saves, 0.0) / GREATEST(COALESCE(feedback.impression_count, 0.0), 1.0), 1.0)::double precision,
                CASE
                    WHEN candidate.content_type = 'route' THEN LEAST(
                        COALESCE(feedback.completions, 0.0)
                            / GREATEST(COALESCE(feedback.joins, 0.0), 1.0)
                            * LEAST(COALESCE(feedback.joins, 0.0) / 20.0, 1.0)
                        + (COALESCE(population.completions, 0.0) + 0.1)
                            / GREATEST(COALESCE(population.joins, 0.0) + 20.0, 20.0)
                            * (1.0 - LEAST(COALESCE(feedback.joins, 0.0) / 20.0, 1.0)),
                        1.0
                    )
                    ELSE LEAST(
                        COALESCE(feedback.completions, 0.0)
                            / GREATEST(COALESCE(feedback.knowledge_starts, 0.0), 1.0)
                            * LEAST(COALESCE(feedback.knowledge_starts, 0.0) / 20.0, 1.0)
                        + (COALESCE(population.completions, 0.0) + 0.1)
                            / GREATEST(COALESCE(population.knowledge_starts, 0.0) + 20.0, 20.0)
                            * (1.0 - LEAST(COALESCE(feedback.knowledge_starts, 0.0) / 20.0, 1.0)),
                        1.0
                    )
                END::double precision,
                LEAST(
                    CASE
                        WHEN candidate.content_type = 'route' THEN
                            COALESCE(feedback.purchases, 0.0)
                                / GREATEST(COALESCE(feedback.impression_count, 0.0), 1.0)
                                * LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0)
                            + (COALESCE(population.purchases, 0.0) + 0.05)
                                / GREATEST(COALESCE(population.impression_count, 0.0) + 50.0, 50.0)
                                * (1.0 - LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0))
                        ELSE 0.0
                    END,
                    1.0
                )::double precision
            FROM content_items AS candidate
            CROSS JOIN normalizers
            LEFT JOIN domain_scores AS domain ON domain.domain = candidate.domain
            LEFT JOIN author_scores AS author ON author.author_id = candidate.author_id
            LEFT JOIN direct_feedback AS feedback ON feedback.content_id = candidate.id
            LEFT JOIN population_feedback AS population ON population.content_id = candidate.id
            WHERE candidate.id = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(content_ids)
        .fetch_all(pool)
        .await;

        match rows {
            Ok(rows) => rows
                .into_iter()
                .map(
                    |(
                        content_id,
                        domain_affinity,
                        author_affinity,
                        impression_fatigue,
                        direct_negative_feedback,
                        click_through_rate,
                        save_rate,
                        action_completion_rate,
                        purchase_conversion_rate,
                    )| {
                        (
                            content_id,
                            CandidateFeatures {
                                domain_affinity,
                                author_affinity,
                                impression_fatigue,
                                direct_negative_feedback,
                                click_through_rate,
                                save_rate,
                                action_completion_rate,
                                purchase_conversion_rate,
                            },
                        )
                    },
                )
                .collect(),
            Err(error) => {
                tracing::warn!(%error, user_id, "candidate feature store degraded");
                HashMap::new()
            }
        }
    }
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

#[derive(Clone)]
pub(crate) struct FeatureCache {
    redis: Option<redis::aio::ConnectionManager>,
    miss_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
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

struct RedisRefreshLease {
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

pub(crate) struct FeatureRefreshGuard {
    _local: tokio::sync::OwnedMutexGuard<()>,
    redis: Option<RedisRefreshLease>,
    peer_holds_lease: bool,
}

impl FeatureRefreshGuard {
    pub(crate) fn peer_holds_lease(&self) -> bool {
        self.peer_holds_lease
    }

    pub(crate) async fn release(mut self) {
        if let Some(lease) = self.redis.take() {
            lease.release().await;
        }
    }
}

impl Drop for FeatureRefreshGuard {
    fn drop(&mut self) {
        let Some(lease) = self.redis.take() else {
            return;
        };
        // The normal path explicitly releases the lease. Drop is a bounded
        // best-effort cleanup; the TTL remains the crash-safety backstop.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(lease.release());
        }
    }
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
