use crate::api::pb;
use bookway_recommend_recall_api::pb::Candidate;

use super::predictor::{MultiObjectivePredictor, ObjectiveEvidence};

/// Versioned ranking-coefficient set. Buckets map onto named weight tables so
/// experiments change real math, not just a label string shipped alongside
/// identical scores (the previous failure mode this replaces).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RankWeights {
    local_score: f64,
    quality_score: f64,
    recall_score: f64,
    freshness: f64,
    recent_positive_rate: f64,
    user_interest_strength: f64,
    negative_feedback_rate: f64,
    domain_affinity: f64,
    author_affinity: f64,
    impression_fatigue: f64,
    direct_negative_feedback: f64,
    p_ctr: f64,
    p_cvr: f64,
    p_wegu: f64,
    route_completion_rate: f64,
    save_rate: f64,
    tag: &'static str,
}

impl RankWeights {
    /// Baseline ("control") table; values are the proven v2 scoring set.
    pub(crate) fn control() -> Self {
        Self {
            local_score: 0.58,
            quality_score: 0.18,
            recall_score: 0.10,
            freshness: 0.08,
            recent_positive_rate: 0.08,
            user_interest_strength: 0.05,
            negative_feedback_rate: 0.16,
            domain_affinity: 0.30,
            author_affinity: 0.22,
            impression_fatigue: 0.32,
            direct_negative_feedback: 0.80,
            // Explicit multi-objective predictions dominate vanity signals. WEGU
            // captures immediate action conversion; route completion is a
            // separate population prior for the whole executable path.
            p_ctr: 0.18,
            p_cvr: 0.20,
            p_wegu: 0.30,
            route_completion_rate: 0.20,
            save_rate: 0.08,
            tag: "w-control",
        }
    }

    /// Variant A: sharpen WEGU/completion weighting further so "适合且做到"
    /// outranks engagement entirely for its cohorts.
    pub(crate) fn wegu_heavy() -> Self {
        let mut weights = Self::control();
        weights.local_score = 0.50;
        weights.quality_score = 0.15;
        weights.recall_score = 0.09;
        weights.freshness = 0.07;
        weights.recent_positive_rate = 0.06;
        weights.user_interest_strength = 0.04;
        weights.negative_feedback_rate = 0.13;
        weights.domain_affinity = 0.27;
        weights.author_affinity = 0.19;
        weights.impression_fatigue = 0.30;
        weights.p_ctr = 0.12;
        weights.p_cvr = 0.16;
        weights.p_wegu = 0.38;
        weights.route_completion_rate = 0.26;
        weights.save_rate = 0.07;
        weights.tag = "w-wegu";
        weights
    }

    /// Variant B: exploration-leaning table paying freshness and saves more to
    /// widen discovery coverage beyond already-engaged domains.
    pub(crate) fn exploration() -> Self {
        let mut weights = Self::control();
        weights.freshness = 0.12;
        weights.save_rate = 0.10;
        weights.route_completion_rate = 0.16;
        weights.p_wegu = 0.26;
        weights.tag = "w-explore";
        weights
    }

    pub(crate) const fn tag(&self) -> &'static str {
        self.tag
    }

    /// Deterministic bucket schedule: buckets 0-4 stay on the baseline so
    /// regressions always have a clean comparison population.
    pub(crate) fn for_bucket(bucket: u8) -> Self {
        match bucket {
            5..=7 => Self::wegu_heavy(),
            8..=9 => Self::exploration(),
            _ => Self::control(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RankingSignals {
    recent_positive_rate: f64,
    user_interest_strength: f64,
    negative_feedback_rate: f64,
}

impl RankingSignals {
    fn from_features(features: Option<&pb::RankFeatures>) -> Self {
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
        }
    }
}

fn objective_evidence(
    candidate: &Candidate,
    features: Option<&pb::RankFeatures>,
) -> ObjectiveEvidence {
    let observed = features.and_then(|features| {
        features
            .candidates
            .iter()
            .find(|entry| entry.content_id == candidate.content_id)
    });
    ObjectiveEvidence {
        explicit_ctr: finite(candidate.p_ctr),
        explicit_cvr: finite(candidate.p_cvr),
        explicit_wegu: finite(candidate.p_wegu),
        observed_ctr: finite(observed.map(|o| o.click_through_rate).unwrap_or_default()),
        observed_cvr: finite(
            observed
                .map(|o| o.purchase_conversion_rate)
                .unwrap_or_default(),
        ),
        observed_wegu: finite(
            observed
                .map(|o| o.action_completion_rate)
                .unwrap_or_default(),
        ),
        route_completion: route_completion_rate(features, &candidate.content_id),
        domain_affinity: observed.map(|o| o.domain_affinity).unwrap_or_default(),
        author_affinity: observed.map(|o| o.author_affinity).unwrap_or_default(),
        impression_fatigue: observed
            .map(|o| o.impression_fatigue)
            .unwrap_or_default(),
        direct_negative_feedback: observed
            .map(|o| o.direct_negative_feedback)
            .unwrap_or_default(),
    }
}

// Per-candidate value helpers; both are window or population rates owned by
// feature-main, so ranking reads them without recomputation.
fn route_completion_rate(features: Option<&pb::RankFeatures>, content_id: &str) -> f64 {
    candidate_field(features, content_id, |c| c.route_completion_rate).clamp(0.0, 1.0)
}

fn save_rate(features: Option<&pb::RankFeatures>, content_id: &str) -> f64 {
    candidate_field(features, content_id, |c| c.save_rate).clamp(0.0, 1.0)
}

fn candidate_field(
    features: Option<&pb::RankFeatures>,
    content_id: &str,
    pick: impl Fn(&pb::CandidateFeatures) -> f64,
) -> f64 {
    features
        .and_then(|features| {
            features
                .candidates
                .iter()
                .find(|entry| entry.content_id == content_id)
        })
        .map(pick)
        .map(finite)
        .unwrap_or_default()
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
    model_version: &str,
    predictor: &dyn MultiObjectivePredictor,
) -> Vec<Candidate> {
    let weights = RankWeights::for_bucket(bucket);
    let signals = RankingSignals::from_features(features);
    for candidate in &mut candidates {
        let candidate_signals =
            CandidateRankingSignals::from_features(features, &candidate.content_id);
        let evidence = objective_evidence(candidate, features);
        // A configured-but-unavailable remote model already defers to its
        // internal heuristic per call and reports degradation on the response.
        let prediction = predictor.predict(&evidence);
        let local_score = finite(candidate.score);
        candidate.score = weights.local_score * local_score
            + weights.quality_score * finite(candidate.quality_score)
            + weights.recall_score * finite(candidate.recall_score)
            + weights.freshness * finite(candidate.freshness)
            + weights.recent_positive_rate * signals.recent_positive_rate
            + weights.user_interest_strength * signals.user_interest_strength
            - weights.negative_feedback_rate * signals.negative_feedback_rate
            + weights.domain_affinity * candidate_signals.domain_affinity
            + weights.author_affinity * candidate_signals.author_affinity
            - weights.impression_fatigue * candidate_signals.impression_fatigue
            - weights.direct_negative_feedback * candidate_signals.direct_negative_feedback;
        candidate.score += weights.p_ctr * prediction.p_ctr
            + weights.p_cvr * prediction.p_cvr
            + weights.p_wegu * prediction.p_wegu
            + weights.route_completion_rate * route_completion_rate(features, &candidate.content_id)
            + weights.save_rate * save_rate(features, &candidate.content_id);
        candidate.p_ctr = prediction.p_ctr;
        candidate.p_cvr = prediction.p_cvr;
        candidate.p_wegu = prediction.p_wegu;
        // Serving-time feature snapshot: the trainer trains on exactly these
        // named values, so the model can never learn from the future.
        candidate.feature_snapshot = [
            ("explicit_ctr", evidence.explicit_ctr),
            ("observed_ctr", evidence.observed_ctr),
            ("observed_cvr", evidence.observed_cvr),
            ("observed_wegu", evidence.observed_wegu),
            ("route_completion", evidence.route_completion),
            ("domain_affinity", evidence.domain_affinity),
            ("author_affinity", evidence.author_affinity),
            ("impression_fatigue", evidence.impression_fatigue),
            ("direct_negative_feedback", evidence.direct_negative_feedback),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_string(), finite(value)))
        .collect();
        if candidate_signals.domain_affinity >= 0.35 || candidate_signals.author_affinity >= 0.35 {
            candidate.reasons.push("符合你近期的行动偏好".to_string());
        }
        if candidate_signals.impression_fatigue > 0.0 {
            candidate.reasons.push("已降低重复曝光".to_string());
        }
        // Machine-facing diagnostics: recommend-main persists these in the
        // exposure ledger but strips the `[debug]` prefix from client reasons.
        candidate
            .reasons
            .push(format!("[debug] {model_version} {} bucket {bucket}", weights.tag()));
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

#[cfg(test)]
mod tests {
    use super::{RankWeights, rank};
    use crate::api::pb;
    use crate::domain::rank::predictor::HeuristicPredictor;
    use bookway_recommend_recall_api::pb::Candidate;

    fn features_for(candidates: Vec<pb::CandidateFeatures>) -> pb::RankFeatures {
        pb::RankFeatures {
            candidates,
            ..Default::default()
        }
    }

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
            "recommend-rank-v2",
            &HeuristicPredictor,
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
            "recommend-rank-v2",
            &HeuristicPredictor,
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
            Some(&features_for(vec![
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
            ])),
            2,
            "recommend-rank-v2",
            &HeuristicPredictor,
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
            Some(&features_for(vec![
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
            ])),
            1,
            "recommend-rank-v2",
            &HeuristicPredictor,
        );

        assert_eq!(ranked[0].content_id, "action-proven");
    }

    #[test]
    fn route_completion_signal_outweighs_a_click_only_candidate() {
        let candidate = |content_id: &str| Candidate {
            content_id: content_id.to_string(),
            score: 1.0,
            ..Default::default()
        };
        let ranked = rank(
            vec![candidate("click-only"), candidate("route-proven")],
            Some(&features_for(vec![
                pb::CandidateFeatures {
                    content_id: "click-only".to_string(),
                    click_through_rate: 1.0,
                    ..Default::default()
                },
                pb::CandidateFeatures {
                    content_id: "route-proven".to_string(),
                    route_completion_rate: 1.0,
                    ..Default::default()
                },
            ])),
            3,
            "recommend-rank-v2",
            &HeuristicPredictor,
        );

        assert_eq!(ranked[0].content_id, "route-proven");
    }

    #[test]
    fn experiment_buckets_select_distinct_weight_tables() {
        assert_eq!(RankWeights::for_bucket(0).tag(), "w-control");
        assert_eq!(RankWeights::for_bucket(4).tag(), "w-control");
        assert_eq!(RankWeights::for_bucket(5).tag(), "w-wegu");
        assert_eq!(RankWeights::for_bucket(7).tag(), "w-wegu");
        assert_eq!(RankWeights::for_bucket(8).tag(), "w-explore");
        assert_eq!(RankWeights::for_bucket(9).tag(), "w-explore");
        // The wegu-heavy table must truly weight WEGU above control's pCTR sum.
        assert!(RankWeights::wegu_heavy().p_wegu > RankWeights::wegu_heavy().p_ctr);
        assert!(RankWeights::exploration().freshness > RankWeights::control().freshness);
    }

    #[test]
    fn ranked_candidates_carry_serving_time_feature_snapshots() {
        let ranked = rank(
            vec![Candidate {
                content_id: "c".to_string(),
                score: 1.0,
                ..Default::default()
            }],
            Some(&pb::RankFeatures {
                candidates: vec![pb::CandidateFeatures {
                    content_id: "c".to_string(),
                    click_through_rate: 0.25,
                    impression_fatigue: 0.4,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            0,
            "recommend-rank-v9",
            &HeuristicPredictor,
        );

        let snapshot = &ranked[0].feature_snapshot;
        assert_eq!(snapshot.get("observed_ctr"), Some(&0.25));
        assert_eq!(snapshot.get("impression_fatigue"), Some(&0.4));
        assert_eq!(snapshot.len(), 9, "the trainer contract fixes the feature set");
    }

    #[test]
    fn ranked_reasons_carry_weight_table_tag_for_diagnostics() {
        let candidate = Candidate {
            content_id: "c".to_string(),
            score: 1.0,
            ..Default::default()
        };
        let ranked = rank(
            vec![candidate],
            None,
            6,
            "recommend-rank-v9",
            &HeuristicPredictor,
        );
        assert!(
            ranked[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("recommend-rank-v9 w-wegu bucket 6"))
        );
    }
}
