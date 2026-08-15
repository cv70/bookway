mod algorithm;

use crate::api::pb;
use crate::domain::Domain;

impl Domain {
    pub(crate) async fn rank(&self, request: pb::RankRequest) -> pb::RankResponse {
        let bucket = algorithm::stable_bucket(&request.user_id);
        pb::RankResponse {
            candidates: algorithm::rank(request.candidates, request.features.as_ref(), bucket),
            model_version: self.model.model_version().to_string(),
            experiment_bucket: format!("{}-{bucket}", self.model.model_version()),
            degraded: false,
        }
    }
}
