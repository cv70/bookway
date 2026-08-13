use std::{collections::HashMap, sync::Arc};

use super::{
    api::{FeatureRequest, FeatureResponse},
    datasource::{FeatureCache, FeatureRepository},
};
use crate::conf::Config;

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    repository: Arc<FeatureRepository>,
    cache: Arc<FeatureCache>,
    model_version: String,
}
impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let pool = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Postgres => Some(bookway_data::postgres_pool().await?),
            bookway_data::StorageMode::Memory => None,
        };
        let redis = match bookway_data::redis_connection().await {
            Ok(redis) => redis,
            Err(error) => {
                tracing::warn!(%error, "redis unavailable at startup; feature cache disabled");
                None
            }
        };
        let model_version = config.model_version.clone();
        Ok(Self {
            config,
            repository: Arc::new(FeatureRepository::new(pool)),
            cache: Arc::new(FeatureCache::new(redis)),
            model_version,
        })
    }

    pub(crate) async fn features(&self, request: FeatureRequest) -> FeatureResponse {
        let mut values = HashMap::from([
            ("user_interest_strength".to_string(), 0.5),
            ("recent_positive_rate".to_string(), 0.2),
            ("negative_feedback_rate".to_string(), 0.0),
        ]);
        let persisted = match self.cache.load(&request.user_id).await {
            Some(features) => features,
            None => {
                let features = self.repository.load(&request.user_id).await;
                self.cache.store(&request.user_id, &features).await;
                features
            }
        };
        values.extend(persisted);
        values.insert(
            "candidate_count".to_string(),
            request.content_ids.len() as f64,
        );
        FeatureResponse {
            user_id: request.user_id,
            model_version: self.model_version.clone(),
            features: values,
        }
    }
}
