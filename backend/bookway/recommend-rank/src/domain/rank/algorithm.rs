use crate::api::pb;
use bookway_recommend_recall_api::pb::Candidate;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RankingSignals {
    recent_positive_rate: f64,
    user_interest_strength: f64,
    negative_feedback_rate: f64,
}

impl RankingSignals {
    pub(crate) fn from_features(features: Option<&pb::RankFeatures>) -> Self {
        Self {
            recent_positive_rate: features
                .map(|features| finite(features.recent_positive_rate))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            user_interest_strength: features
                .map(|features| finite(features.user_interest_strength))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            negative_feedback_rate: features
                .map(|features| finite(features.negative_feedback_rate))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CandidateRankingSignals {
    domain_affinity: f64,
    author_affinity: f64,
    impression_fatigue: f64,
    direct_negative_feedback: f64,
    save_rate: f64,
    p_ctr: f64,
    p_cvr: f64,
    p_wegu: f64,
}

impl CandidateRankingSignals {
    fn from_features(features: Option<&pb::RankFeatures>, content_id: &str) -> Self {
        let candidate = features.and_then(|features| {
            features
                .candidates
                .iter()
                .find(|candidate| candidate.content_id == content_id)
        });
        Self {
            domain_affinity: candidate
                .map(|candidate| finite(candidate.domain_affinity))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            author_affinity: candidate
                .map(|candidate| finite(candidate.author_affinity))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            impression_fatigue: candidate
                .map(|candidate| finite(candidate.impression_fatigue))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            direct_negative_feedback: candidate
                .map(|candidate| finite(candidate.direct_negative_feedback))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            save_rate: candidate
                .map(|candidate| finite(candidate.save_rate))
                .unwrap_or_default()
                .clamp(0.0, 1.0),
            p_ctr: candidate
                .map(|candidate| predicted(candidate.p_ctr, candidate.click_through_rate))
                .unwrap_or(0.0001),
            p_cvr: candidate
                .map(|candidate| predicted(candidate.p_cvr, candidate.purchase_conversion_rate))
                .unwrap_or(0.0001),
            p_wegu: candidate
                .map(|candidate| predicted(candidate.p_wegu, candidate.action_completion_rate))
                .unwrap_or(0.0001),
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
    mut candidates: Vec<Candidate>,
    features: Option<&pb::RankFeatures>,
    bucket: u8,
) -> Vec<Candidate> {
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
        // Explicit multi-objective predictions dominate vanity signals:
        // WEGU (action completion) carries the largest online objective weight.
        candidate.score += 0.18 * candidate_signals.p_ctr
            + 0.20 * candidate_signals.p_cvr
            + 0.42 * candidate_signals.p_wegu
            + 0.08 * candidate_signals.save_rate;
        candidate.p_ctr = candidate_signals.p_ctr;
        candidate.p_cvr = candidate_signals.p_cvr;
        candidate.p_wegu = candidate_signals.p_wegu;
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

fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn predicted(explicit: f64, observed_rate: f64) -> f64 {
    let explicit = finite(explicit);
    if explicit > 0.0 {
        explicit.clamp(0.0001, 1.0)
    } else {
        finite(observed_rate).clamp(0.0001, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::rank;
    use crate::api::pb;
    use bookway_recommend_recall_api::pb::Candidate;

    #[test]
    fn negative_feedback_penalizes_scores_and_non_finite_inputs_are_safe() {
        let candidate = Candidate {
            content_id: "content-1".to_string(),
            quality_score: 1.0,
            freshness: 1.0,
            recall_score: 1.0,
            score: f64::NAN,
            ..Default::default()
        };
        let positive = rank(
            vec![candidate.clone()],
            Some(&pb::RankFeatures {
                recent_positive_rate: 1.0,
                user_interest_strength: 1.0,
                negative_feedback_rate: 0.0,
                candidates: Vec::new(),
            }),
            0,
        );
        let negative = rank(
            vec![candidate],
            Some(&pb::RankFeatures {
                recent_positive_rate: 0.0,
                user_interest_strength: 0.0,
                negative_feedback_rate: 1.0,
                candidates: Vec::new(),
            }),
            0,
        );

        assert!(positive[0].score.is_finite());
        assert!(positive[0].score > negative[0].score);
    }

    #[test]
    fn candidate_affinity_and_feedback_change_relative_order() {
        let candidate = |content_id: &str| Candidate {
            content_id: content_id.to_string(),
            score: 1.0,
            ..Default::default()
        };
        let ranked = rank(
            vec![candidate("preferred"), candidate("reported")],
            Some(&pb::RankFeatures {
                recent_positive_rate: 0.0,
                user_interest_strength: 0.0,
                negative_feedback_rate: 0.0,
                candidates: vec![
                    pb::CandidateFeatures {
                        content_id: "preferred".to_string(),
                        domain_affinity: 1.0,
                        author_affinity: 0.5,
                        impression_fatigue: 0.0,
                        direct_negative_feedback: 0.0,
                        ..Default::default()
                    },
                    pb::CandidateFeatures {
                        content_id: "reported".to_string(),
                        domain_affinity: 0.0,
                        author_affinity: 0.0,
                        impression_fatigue: 0.0,
                        direct_negative_feedback: 1.0,
                        ..Default::default()
                    },
                ],
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

    #[test]
    fn completion_signal_outweighs_a_click_only_candidate() {
        let candidate = |content_id: &str| Candidate {
            content_id: content_id.to_string(),
            score: 1.0,
            ..Default::default()
        };
        let ranked = rank(
            vec![candidate("click-only"), candidate("action-proven")],
            Some(&pb::RankFeatures {
                candidates: vec![
                    pb::CandidateFeatures {
                        content_id: "click-only".to_string(),
                        click_through_rate: 1.0,
                        ..Default::default()
                    },
                    pb::CandidateFeatures {
                        content_id: "action-proven".to_string(),
                        action_completion_rate: 1.0,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            1,
        );

        assert_eq!(ranked[0].content_id, "action-proven");
    }
}
