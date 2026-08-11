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
        let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT feature_name,value FROM user_features WHERE user_id=$1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await;
        match rows {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|(name, value)| value.as_f64().map(|number| (name, number)))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, user_id, "feature store degraded");
                HashMap::new()
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
