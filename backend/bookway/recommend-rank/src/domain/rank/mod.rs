mod algorithm;

use crate::api::pb;
use crate::domain::Domain;

impl Domain {
    pub(crate) async fn rank(&self, request: pb::RankRequest) -> pb::RankResponse {
        let features: serde_json::Value =
            serde_json::from_str(&request.features_json).unwrap_or_default();
        let bucket = algorithm::stable_bucket(&request.user_id);
        pb::RankResponse {
            candidates: algorithm::rank(
                request.candidates,
                algorithm::RankingSignals::from_features(&features),
                bucket,
            ),
            model_version: self.model.model_version().to_string(),
            experiment_bucket: format!("{}-{bucket}", self.model.model_version()),
            degraded: false,
        }
    }
}
