use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct FeatureRepository {
    pool: Option<sqlx::PgPool>,
}
impl FeatureRepository {
    pub(crate) fn new(pool: Option<sqlx::PgPool>) -> Self {
        Self { pool }
    }
    pub(crate) async fn load(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        // Keep feature freshness bounded while deriving feedback features
        // from the canonical event log. The event types are intentionally
        // weighted so hides reduce exploration and positive actions increase
        // affinity without letting a single event dominate.
        let feedback = sqlx::query_as::<_, (i64, i64, i64, i64)>(
            "SELECT COUNT(*) FILTER (WHERE event_type IN ('like','bookmark','share','complete')), COUNT(*) FILTER (WHERE event_type = 'hide'), COUNT(*) FILTER (WHERE event_type IN ('impression','view')), COUNT(*) FROM user_events WHERE user_id=$1 AND occurred_at > now() - interval '30 days'",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or((0, 0, 0, 0));
        let positive = feedback.0 as f64;
        let negative = feedback.1 as f64;
        let impressions = feedback.2 as f64;
        let total = feedback.3 as f64;
        let mut derived = HashMap::from([
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
}

#[derive(Clone)]
pub(crate) struct FeatureCache {
    redis: Option<redis::aio::ConnectionManager>,
}
impl FeatureCache {
    pub(crate) fn new(redis: Option<redis::aio::ConnectionManager>) -> Self {
        Self { redis }
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
