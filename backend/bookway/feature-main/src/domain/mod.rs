use std::{collections::HashMap, sync::Arc};

use super::{
    api::{FeatureRequest, FeatureResponse},
    datasource::{CandidateFeatures, FeatureCache, FeatureRepository},
};
use crate::conf::Config;

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    repository: Arc<FeatureRepository>,
    cache: Arc<FeatureCache>,
    model_version: String,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
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
        let user_features = async {
            match self.cache.load(&request.user_id).await {
                Some(features) => features,
                None => {
                    let features = self.repository.load(&request.user_id).await;
                    self.cache.store(&request.user_id, &features).await;
                    features
                }
            }
        };
        let candidate_features = self
            .repository
            .load_candidates(&request.user_id, &request.content_ids);
        let (persisted, candidate_features) = tokio::join!(user_features, candidate_features);
        values.extend(persisted);
        values.insert(
            "candidate_count".to_string(),
            request.content_ids.len() as f64,
        );
        FeatureResponse {
            user_id: request.user_id,
            model_version: self.model_version.clone(),
            features: feature_payload(values, candidate_features),
        }
    }
}

fn feature_payload(
    values: HashMap<String, f64>,
    candidate_features: HashMap<String, CandidateFeatures>,
) -> serde_json::Value {
    let mut payload = serde_json::to_value(values).unwrap_or_default();
    if let Some(payload) = payload.as_object_mut() {
        payload.insert(
            "candidates".to_string(),
            serde_json::to_value(candidate_features).unwrap_or_default(),
        );
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_payload_keeps_user_and_candidate_signals() {
        let payload = feature_payload(
            HashMap::from([("recent_positive_rate".to_string(), 0.4)]),
            HashMap::from([(
                "content-1".to_string(),
                CandidateFeatures {
                    domain_affinity: 0.8,
                    author_affinity: 0.3,
                    impression_fatigue: 0.5,
                    direct_negative_feedback: 0.0,
                },
            )]),
        );

        assert_eq!(payload["recent_positive_rate"], 0.4);
        assert_eq!(payload["candidates"]["content-1"]["domain_affinity"], 0.8);
    }
}
