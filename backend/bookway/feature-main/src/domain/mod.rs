use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::conf::Config;
use crate::{
    api::pb,
    datasource::{FeatureDao, UserFeatureCache, user_feature_cache},
};

const MAX_CANDIDATE_IDS: usize = 500;

#[cfg(test)]
use crate::datasource::CandidateFeatures;

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    dao: Arc<FeatureDao>,
    cache: Arc<UserFeatureCache>,
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
            dao: Arc::new(FeatureDao::new(pool, model_version.clone())),
            cache: Arc::new(user_feature_cache(redis)),
            model_version,
        })
    }

    pub(crate) async fn features(&self, request: pb::FeaturesRequest) -> pb::FeaturesResponse {
        let user_id = request.user_id.trim().to_string();
        let content_ids = normalize_content_ids(&request.content_ids);
        // Empty identity is the anonymous lane. Never use it as a cache or
        // database key: doing so would share one user's feature namespace with
        // every anonymous request. Anonymous callers receive only safe priors.
        if user_id.is_empty() {
            return anonymous_features_response(&self.model_version);
        }
        let mut values = HashMap::from([
            ("user_interest_strength".to_string(), 0.5),
            ("recent_positive_rate".to_string(), 0.2),
            ("negative_feedback_rate".to_string(), 0.0),
        ]);
        let user_features = async {
            match self.cache.load(&user_id).await {
                Some(features) => features,
                None => {
                    // Serialize misses for the same user. The second read
                    // avoids a PostgreSQL stampede when a hot Redis key
                    // expires under concurrent recommendation requests.
                    let miss_guard = self.cache.refresh_lock(&user_id).await;
                    if let Some(features) = self.cache.load(&user_id).await {
                        miss_guard.release().await;
                        return features;
                    }
                    if miss_guard.peer_holds_lease() {
                        miss_guard.release().await;
                        return HashMap::new();
                    }
                    let features = self.dao.load(&user_id).await;
                    self.cache.store(&user_id, &features).await;
                    miss_guard.release().await;
                    features
                }
            }
        };
        let candidate_features = self.dao.load_candidates(&user_id, &content_ids);
        let (persisted, candidate_features) = tokio::join!(user_features, candidate_features);
        values.extend(persisted);
        values.insert("candidate_count".to_string(), content_ids.len() as f64);
        pb::FeaturesResponse {
            user_id,
            model_version: self.model_version.clone(),
            recent_positive_rate: value(&values, "recent_positive_rate"),
            user_interest_strength: value(&values, "user_interest_strength"),
            negative_feedback_rate: value(&values, "negative_feedback_rate"),
            learning_interest: value(&values, "domain_interest.learning"),
            movement_interest: value(&values, "domain_interest.movement"),
            wellness_interest: value(&values, "domain_interest.wellness"),
            travel_interest: value(&values, "domain_interest.travel"),
            leisure_interest: value(&values, "domain_interest.leisure"),
            candidates: candidate_features
                .into_iter()
                .map(|(content_id, features)| pb::CandidateFeatures {
                    content_id,
                    domain_affinity: features.domain_affinity,
                    author_affinity: features.author_affinity,
                    impression_fatigue: features.impression_fatigue,
                    direct_negative_feedback: features.direct_negative_feedback,
                    click_through_rate: features.click_through_rate,
                    save_rate: features.save_rate,
                    action_completion_rate: features.action_completion_rate,
                    purchase_conversion_rate: features.purchase_conversion_rate,
                    route_completion_rate: features.route_completion_rate,
                })
                .collect(),
        }
    }
}

fn normalize_content_ids(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(*value))
        .take(MAX_CANDIDATE_IDS)
        .map(str::to_string)
        .collect()
}

fn value(values: &HashMap<String, f64>, name: &str) -> f64 {
    values
        .get(name)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or_default()
}

fn anonymous_features_response(model_version: &str) -> pb::FeaturesResponse {
    pb::FeaturesResponse {
        model_version: model_version.to_string(),
        recent_positive_rate: 0.2,
        user_interest_strength: 0.5,
        negative_feedback_rate: 0.0,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_features_use_priors_without_a_user_namespace() {
        let response = anonymous_features_response("feature-v1");
        assert!(response.user_id.is_empty());
        assert_eq!(response.model_version, "feature-v1");
        assert_eq!(response.recent_positive_rate, 0.2);
        assert_eq!(response.user_interest_strength, 0.5);
        assert!(response.candidates.is_empty());
    }

    #[test]
    fn builds_typed_feature_response() {
        let values = HashMap::from([("recent_positive_rate".to_string(), 0.4)]);
        let response = pb::FeaturesResponse {
            recent_positive_rate: value(&values, "recent_positive_rate"),
            ..Default::default()
        };

        assert_eq!(response.recent_positive_rate, 0.4);
    }

    #[test]
    fn candidate_features_map_to_the_protobuf_contract() {
        let candidates = HashMap::from([(
            "content-1".to_string(),
            CandidateFeatures {
                domain_affinity: 0.8,
                author_affinity: 0.3,
                impression_fatigue: 0.5,
                direct_negative_feedback: 0.0,
                ..Default::default()
            },
        )]);
        let response = pb::FeaturesResponse {
            candidates: candidates
                .into_iter()
                .map(|(content_id, features)| pb::CandidateFeatures {
                    content_id,
                    domain_affinity: features.domain_affinity,
                    author_affinity: features.author_affinity,
                    impression_fatigue: features.impression_fatigue,
                    direct_negative_feedback: features.direct_negative_feedback,
                    click_through_rate: features.click_through_rate,
                    save_rate: features.save_rate,
                    action_completion_rate: features.action_completion_rate,
                    purchase_conversion_rate: features.purchase_conversion_rate,
                    route_completion_rate: features.route_completion_rate,
                })
                .collect(),
            ..Default::default()
        };

        assert_eq!(response.candidates[0].domain_affinity, 0.8);
    }

    #[test]
    fn feature_values_remain_finite() {
        let values = HashMap::from([("recent_positive_rate".to_string(), f64::NAN)]);
        assert_eq!(value(&values, "recent_positive_rate"), 0.0);
    }

    #[test]
    fn feature_defaults_are_zero() {
        let values = HashMap::new();
        assert_eq!(value(&values, "recent_positive_rate"), 0.0);
    }

    #[test]
    fn candidate_ids_are_normalized_and_bounded_before_feature_lookup() {
        let mut ids = vec![
            " content-1 ".to_string(),
            "content-1".to_string(),
            "".to_string(),
        ];
        ids.extend((0..(MAX_CANDIDATE_IDS + 20)).map(|index| format!("content-{index}")));

        let normalized = normalize_content_ids(&ids);

        assert_eq!(normalized.len(), MAX_CANDIDATE_IDS);
        assert_eq!(normalized.first().map(String::as_str), Some("content-1"));
        assert!(
            normalized
                .iter()
                .all(|id| !id.is_empty() && id.trim() == id)
        );
        assert_eq!(
            normalized.iter().collect::<HashSet<_>>().len(),
            normalized.len()
        );
    }
}
