use std::{collections::HashMap, sync::Arc};

use super::{
    api::{FeatureRequest, FeatureResponse},
    datasource::{FeatureCache, FeatureRepository},
};

#[derive(Clone)]
pub(crate) struct FeatureService {
    repository: Arc<FeatureRepository>,
    cache: Arc<FeatureCache>,
    model_version: String,
}
impl FeatureService {
    pub(crate) fn new(
        repository: Arc<FeatureRepository>,
        cache: Arc<FeatureCache>,
        model_version: String,
    ) -> Self {
        Self {
            repository,
            cache,
            model_version,
        }
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
