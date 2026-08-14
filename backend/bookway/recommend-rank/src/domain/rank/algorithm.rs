use crate::api::pb;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RankingSignals {
    recent_positive_rate: f64,
    user_interest_strength: f64,
    negative_feedback_rate: f64,
}

impl RankingSignals {
    pub(crate) fn from_features(features: &serde_json::Value) -> Self {
        Self {
            recent_positive_rate: feature(features, "recent_positive_rate"),
            user_interest_strength: feature(features, "user_interest_strength"),
            negative_feedback_rate: feature(features, "negative_feedback_rate"),
        }
    }
}

pub(crate) fn stable_bucket(value: &str) -> u8 {
    value
        .bytes()
        .fold(0_u8, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte))
        % 10
}
pub(crate) fn rank(
    mut candidates: Vec<pb::Candidate>,
    signals: RankingSignals,
    bucket: u8,
) -> Vec<pb::Candidate> {
    for candidate in &mut candidates {
        let local_score = finite(candidate.score);
        candidate.score = 0.58 * local_score
            + 0.18 * finite(candidate.quality_score)
            + 0.10 * finite(candidate.recall_score)
            + 0.08 * finite(candidate.freshness)
            + 0.08 * signals.recent_positive_rate
            + 0.05 * signals.user_interest_strength
            - 0.16 * signals.negative_feedback_rate;
        candidate
            .reasons
            .push(format!("recommend-rank-v2 bucket {bucket}"));
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    candidates
}

fn feature(features: &serde_json::Value, name: &str) -> f64 {
    features
        .get(name)
        .and_then(serde_json::Value::as_f64)
        .map(finite)
        .unwrap_or_default()
        .clamp(0.0, 1.0)
}

fn finite(value: f64) -> f64 {
    value.is_finite().then_some(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{RankingSignals, rank};
    use crate::api::pb;

    #[test]
    fn negative_feedback_penalizes_scores_and_non_finite_inputs_are_safe() {
        let candidate = pb::Candidate {
            content_id: "content-1".to_string(),
            quality_score: 1.0,
            freshness: 1.0,
            recall_score: 1.0,
            score: f64::NAN,
            ..Default::default()
        };
        let positive = rank(
            vec![candidate.clone()],
            RankingSignals::from_features(&serde_json::json!({
                "recent_positive_rate": 1.0,
                "user_interest_strength": 1.0,
                "negative_feedback_rate": 0.0,
            })),
            0,
        );
        let negative = rank(
            vec![candidate],
            RankingSignals::from_features(&serde_json::json!({
                "negative_feedback_rate": 1.0,
            })),
            0,
        );

        assert!(positive[0].score.is_finite());
        assert!(positive[0].score > negative[0].score);
    }
}
