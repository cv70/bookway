use super::api::{RankRequest, RankedItem};

pub(crate) struct HeuristicRanker {
    model_version: String,
}

impl HeuristicRanker {
    pub(crate) fn new(model_version: String) -> Self {
        Self { model_version }
    }

    pub(crate) fn rank(&self, request: RankRequest) -> Vec<RankedItem> {
        let interest = request
            .features
            .get("user_interest_strength")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.5);
        let bucket = format!("{}-{}", self.model_version, stable_bucket(&request.user_id));
        let mut ranked = request
            .candidates
            .into_iter()
            .map(|candidate| RankedItem {
                content_id: candidate.content_id,
                score: 0.45 * candidate.recall_score
                    + 0.35 * candidate.quality_score
                    + 0.15 * candidate.freshness
                    + 0.05 * interest,
                model_version: self.model_version.clone(),
                experiment_bucket: bucket.clone(),
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.content_id.cmp(&right.content_id))
        });
        ranked
    }
}

fn stable_bucket(user_id: &str) -> u8 {
    user_id
        .bytes()
        .fold(0_u8, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte))
        % 10
}
