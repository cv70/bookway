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

#[derive(Clone, Copy, Debug, Default)]
struct CandidateRankingSignals {
    domain_affinity: f64,
    author_affinity: f64,
    impression_fatigue: f64,
    direct_negative_feedback: f64,
}

impl CandidateRankingSignals {
    fn from_features(features: &serde_json::Value, content_id: &str) -> Self {
        let candidate = features
            .get("candidates")
            .and_then(|items| items.get(content_id));
        Self {
            domain_affinity: nested_feature(candidate, "domain_affinity"),
            author_affinity: nested_feature(candidate, "author_affinity"),
            impression_fatigue: nested_feature(candidate, "impression_fatigue"),
            direct_negative_feedback: nested_feature(candidate, "direct_negative_feedback"),
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
    features: &serde_json::Value,
    bucket: u8,
) -> Vec<pb::Candidate> {
    let signals = RankingSignals::from_features(features);
    for candidate in &mut candidates {
        let candidate_signals =
            CandidateRankingSignals::from_features(features, &candidate.content_id);
        let local_score = finite(candidate.score);
        candidate.score = 0.58 * local_score
            + 0.18 * finite(candidate.quality_score)
            + 0.10 * finite(candidate.recall_score)
            + 0.08 * finite(candidate.freshness)
            + 0.08 * signals.recent_positive_rate
            + 0.05 * signals.user_interest_strength
            - 0.16 * signals.negative_feedback_rate
            + 0.30 * candidate_signals.domain_affinity
            + 0.22 * candidate_signals.author_affinity
            - 0.32 * candidate_signals.impression_fatigue
            - 0.80 * candidate_signals.direct_negative_feedback;
        if candidate_signals.domain_affinity >= 0.35 || candidate_signals.author_affinity >= 0.35 {
            candidate.reasons.push("符合你近期的行动偏好".to_string());
        }
        if candidate_signals.impression_fatigue > 0.0 {
            candidate.reasons.push("已降低重复曝光".to_string());
        }
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

fn nested_feature(features: Option<&serde_json::Value>, name: &str) -> f64 {
    features
        .and_then(|features| features.get(name))
        .and_then(serde_json::Value::as_f64)
        .map(finite)
        .unwrap_or_default()
        .clamp(0.0, 1.0)
}

fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::rank;
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
            &serde_json::json!({
                "recent_positive_rate": 1.0,
                "user_interest_strength": 1.0,
                "negative_feedback_rate": 0.0,
            }),
            0,
        );
        let negative = rank(
            vec![candidate],
            &serde_json::json!({
                "negative_feedback_rate": 1.0,
            }),
            0,
        );

        assert!(positive[0].score.is_finite());
        assert!(positive[0].score > negative[0].score);
    }

    #[test]
    fn candidate_affinity_and_feedback_change_relative_order() {
        let candidate = |content_id: &str| pb::Candidate {
            content_id: content_id.to_string(),
            score: 1.0,
            ..Default::default()
        };
        let ranked = rank(
            vec![candidate("preferred"), candidate("reported")],
            &serde_json::json!({
                "candidates": {
                    "preferred": { "domain_affinity": 1.0, "author_affinity": 0.5 },
                    "reported": { "direct_negative_feedback": 1.0 }
                }
            }),
            2,
        );

        assert_eq!(ranked[0].content_id, "preferred");
        assert!(
            ranked[0]
                .reasons
                .iter()
                .any(|reason| reason == "符合你近期的行动偏好")
        );
        assert!(ranked[0].score > ranked[1].score);
    }
}
