use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tonic::transport::Channel;

use super::{Candidate, CandidateRanker, FeedQuery, PipelineError, RankOutcome};
use bookway_feature_main_api::pb::{self as feature, feature_main_client::FeatureMainClient};
use bookway_recommend_rank_api::pb as rank;
use bookway_bbs_link_api::pb::GrowthDomain;
use bookway_recommend_recall_api::pb as recall;

pub(crate) struct RecommendRanker {
    ranker: Arc<rank::recommend_rank_client::RecommendRankClient<Channel>>,
    feature_client: feature::feature_main_client::FeatureMainClient<Channel>,
}
impl RecommendRanker {
    pub(crate) fn new(
        ranker: Arc<rank::recommend_rank_client::RecommendRankClient<Channel>>,
        feature_client: FeatureMainClient<Channel>,
    ) -> Self {
        Self {
            ranker,
            feature_client,
        }
    }

    async fn request_features(
        &self,
        user_id: &str,
        candidates: &[Candidate],
    ) -> Result<feature::FeaturesResponse, PipelineError> {
        let ids: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        let mut feature_client = self.feature_client.clone();
        let response = feature_client
            .features(
                bookway_runtime::grpc_service_request(feature::FeaturesRequest {
                    user_id: user_id.to_string(),
                    content_ids: ids,
                })
                .map_err(|error| PipelineError::Model(error.to_string()))?,
            )
            .await
            .map_err(|error| PipelineError::Model(error.to_string()))?;
        Ok(response.into_inner())
    }
}

#[async_trait]
impl CandidateRanker for RecommendRanker {
    async fn rank(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<RankOutcome, PipelineError> {
        let user_id = query.user_id_or_empty();
        let features = self.request_features(user_id, candidates).await?;
        let rank_features = rank_features(features.clone());
        let mut client = (*self.ranker).clone();
        let response = match client
            .rank(
                bookway_runtime::grpc_service_request(rank::RankRequest {
                    user_id: user_id.to_string(),
                    features: Some(rank_features),
                    candidates: candidates.iter().map(candidate_to_proto).collect(),
                    user_context: user_context_text(query),
                })
                .map_err(|error| PipelineError::Model(error.to_string()))?,
            )
            .await
        {
            Ok(response) => response.into_inner(),
            Err(error) => {
                // The feature read is already bounded and contains the same
                // calibrated objectives as the model contract. Preserve
                // action-oriented ordering during a model outage instead of
                // falling back to a click/popularity-only heuristic.
                apply_feature_fallback(candidates, &features);
                tracing::warn!(%error, "recommend-rank unavailable; applied local multi-objective fallback");
                return Ok(RankOutcome {
                    model_version: Some("recommend-rank-feature-fallback-v1".to_string()),
                    experiment_bucket: None,
                    degraded: true,
                });
            }
        };
        let expected_scores = candidates.len();
        let scores = response
            .candidates
            .into_iter()
            .map(|candidate| {
                let objectives = (candidate.p_ctr, candidate.p_cvr, candidate.p_wegu);
                (
                    candidate.content_id,
                    (candidate.score, objectives, candidate.feature_snapshot),
                )
            })
            .collect::<HashMap<_, _>>();
        let scored_candidates = apply_ranked_scores(candidates, scores);
        Ok(RankOutcome {
            model_version: (!response.model_version.is_empty()).then_some(response.model_version),
            experiment_bucket: (!response.experiment_bucket.is_empty())
                .then_some(response.experiment_bucket),
            degraded: response.degraded || scored_candidates != expected_scores,
        })
    }
}

/// Applies the rank response's fused score and per-objective estimates to the
/// matching candidates. Returns how many candidates the response covered.
fn apply_ranked_scores(
    candidates: &mut [Candidate],
    scores: HashMap<String, (f64, (f64, f64, f64), HashMap<String, f64>)>,
) -> usize {
    let mut scored_candidates = 0;
    for candidate in candidates {
        if let Some((score, (p_ctr, p_cvr, p_wegu), feature_snapshot)) =
            scores.get(&candidate.post.id)
        {
            scored_candidates += 1;
            candidate.score = *score;
            // Keep the per-objective estimates and the serving-time feature
            // values so the exposure ledger can record exactly what the ranker
            // used and predicted for this serving.
            candidate.p_ctr = *p_ctr;
            candidate.p_cvr = *p_cvr;
            candidate.p_wegu = *p_wegu;
            candidate.feature_snapshot = feature_snapshot.clone();
            candidate.reasons.push("模型排序".to_string());
        }
    }
    scored_candidates
}

fn apply_feature_fallback(candidates: &mut [Candidate], features: &feature::FeaturesResponse) {
    for candidate in candidates {
        let Some(signal) = features
            .candidates
            .iter()
            .find(|signal| signal.content_id == candidate.post.id)
        else {
            continue;
        };
        let p_ctr = calibrated_probability(signal.click_through_rate);
        let p_cvr = calibrated_probability(signal.purchase_conversion_rate);
        let p_wegu = calibrated_probability(signal.action_completion_rate);
        let route_completion = finite_probability(signal.route_completion_rate);
        candidate.p_ctr = p_ctr;
        candidate.p_cvr = p_cvr;
        candidate.p_wegu = p_wegu;
        candidate.score += 0.18 * p_ctr
            + 0.20 * p_cvr
            + 0.30 * p_wegu
            + 0.20 * route_completion
            + 0.08 * finite_probability(signal.save_rate);
        candidate
            .reasons
            .push("[debug] 多目标特征降级排序".to_string());
    }
}

/// Human-readable serving context for the LLM scorer, composed only from
/// hydrated request facts (declared interests, surface). No private ledger
/// text ever leaves the pipeline boundary through this field.
fn user_context_text(query: &FeedQuery) -> String {
    // Labels keyed by bbs-link GrowthDomain's real discriminants
    // (Learning=0 .. Leisure=4) — an off-by-one table once told the LLM a
    // movement candidate was "学习".
    const DOMAIN_LABELS: &[(i32, &str)] = &[
        (GrowthDomain::Learning as i32, "学习"),
        (GrowthDomain::Movement as i32, "运动"),
        (GrowthDomain::Wellness as i32, "健康"),
        (GrowthDomain::Travel as i32, "旅行"),
        (GrowthDomain::Leisure as i32, "休闲"),
    ];
    let mut interests = query
        .interests
        .iter()
        .filter_map(|domain| {
            DOMAIN_LABELS
                .iter()
                .find(|(value, _)| *value == *domain as i32)
                .map(|(_, label)| *label)
        })
        .collect::<Vec<_>>();
    interests.sort();
    interests.dedup();
    let interests_text = if interests.is_empty() {
        "暂无明确兴趣".to_string()
    } else {
        interests.join("、")
    };
    let surface_text = match query.surface.as_str() {
        "following" => "关注流",
        _ => "发现流",
    };
    format!("用户兴趣领域：{interests_text}；内容场景：{surface_text}")
}

fn rank_features(features: feature::FeaturesResponse) -> rank::RankFeatures {
    rank::RankFeatures {
        recent_positive_rate: features.recent_positive_rate,
        user_interest_strength: features.user_interest_strength,
        negative_feedback_rate: features.negative_feedback_rate,
        candidates: features
            .candidates
            .into_iter()
            .map(|candidate| rank::CandidateFeatures {
                content_id: candidate.content_id,
                domain_affinity: candidate.domain_affinity,
                author_affinity: candidate.author_affinity,
                impression_fatigue: candidate.impression_fatigue,
                direct_negative_feedback: candidate.direct_negative_feedback,
                click_through_rate: candidate.click_through_rate,
                save_rate: candidate.save_rate,
                action_completion_rate: candidate.action_completion_rate,
                purchase_conversion_rate: candidate.purchase_conversion_rate,
                p_ctr: calibrated_probability(candidate.click_through_rate),
                p_cvr: calibrated_probability(candidate.purchase_conversion_rate),
                p_wegu: calibrated_probability(candidate.action_completion_rate),
                route_completion_rate: calibrated_probability(candidate.route_completion_rate),
            })
            .collect(),
    }
}

fn candidate_to_proto(candidate: &Candidate) -> recall::Candidate {
    recall::Candidate {
        content_id: candidate.post.id.clone(),
        post: Some(candidate.post.clone()),
        author_id: candidate.author_id.clone(),
        status: candidate.status,
        quality_score: candidate.quality_score,
        freshness: candidate.post.freshness,
        recall_score: candidate.recall_score,
        score: candidate.score,
        p_ctr: candidate.p_ctr,
        p_cvr: candidate.p_cvr,
        p_wegu: candidate.p_wegu,
        feature_snapshot: candidate.feature_snapshot.clone(),
        source: candidate.source.clone(),
        reasons: candidate.reasons.clone(),
    }
}

fn calibrated_probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0001, 1.0)
    } else {
        0.0001
    }
}

fn finite_probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bookway_bbs_link_api::pb::PostSummary;

    use super::{apply_feature_fallback, apply_ranked_scores, candidate_to_proto};
    use crate::domain::pipeline::Candidate;
    use bookway_feature_main_api::pb as feature;

    #[test]
    fn preserves_recall_evidence_after_heuristic_scoring() {
        let candidate = Candidate {
            post: PostSummary {
                id: "content-1".to_string(),
                ..Default::default()
            },
            recall_score: 0.25,
            score: 0.92,
            ..test_candidate_defaults()
        };

        let request_candidate = candidate_to_proto(&candidate);
        assert_eq!(request_candidate.recall_score, 0.25);
        assert_eq!(request_candidate.score, 0.92);
    }

    #[test]
    fn rank_response_objectives_land_on_candidates_for_the_exposure_ledger() {
        let mut candidates = vec![
            Candidate {
                post: PostSummary {
                    id: "content-1".to_string(),
                    ..Default::default()
                },
                ..test_candidate_defaults()
            },
            Candidate {
                post: PostSummary {
                    id: "content-2".to_string(),
                    ..Default::default()
                },
                ..test_candidate_defaults()
            },
        ];
        let scores = HashMap::from([
            (
                "content-1".to_string(),
                (
                    0.9,
                    (0.11, 0.07, 0.33),
                    HashMap::from([("observed_ctr".to_string(), 0.4)]),
                ),
            ),
            // content-2 is deliberately missing: the ranker did not cover it.
        ]);

        let scored = apply_ranked_scores(&mut candidates, scores);

        assert_eq!(scored, 1);
        assert_eq!(candidates[0].score, 0.9);
        assert_eq!(candidates[0].p_ctr, 0.11);
        assert_eq!(candidates[0].p_cvr, 0.07);
        assert_eq!(candidates[0].p_wegu, 0.33);
        assert_eq!(
            candidates[0].feature_snapshot.get("observed_ctr"),
            Some(&0.4),
            "the serving-time feature values ride to the exposure ledger"
        );
        assert_eq!(
            candidates[1].p_wegu, 0.0,
            "uncovered candidates keep zeroed objectives so the ledger can spot ranker gaps"
        );
        assert_eq!(
            candidates[0].reasons.last().map(String::as_str),
            Some("模型排序")
        );
    }

    #[test]
    fn local_fallback_keeps_action_completion_a_first_class_signal() {
        let mut candidates = vec![
            Candidate {
                post: PostSummary {
                    id: "click-only".to_string(),
                    ..Default::default()
                },
                score: 1.0,
                ..test_candidate_defaults()
            },
            Candidate {
                post: PostSummary {
                    id: "action-proven".to_string(),
                    ..Default::default()
                },
                score: 1.0,
                ..test_candidate_defaults()
            },
        ];
        apply_feature_fallback(
            &mut candidates,
            &feature::FeaturesResponse {
                candidates: vec![
                    feature::CandidateFeatures {
                        content_id: "click-only".to_string(),
                        click_through_rate: 1.0,
                        ..Default::default()
                    },
                    feature::CandidateFeatures {
                        content_id: "action-proven".to_string(),
                        action_completion_rate: 1.0,
                        route_completion_rate: 1.0,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        );
        assert_eq!(
            candidates
                .iter()
                .max_by(|left, right| left.score.total_cmp(&right.score))
                .map(|candidate| candidate.post.id.as_str()),
            Some("action-proven")
        );
        assert!(candidates.iter().all(|candidate| {
            candidate
                .reasons
                .iter()
                .any(|reason| reason == "[debug] 多目标特征降级排序")
        }));
    }

    fn test_candidate_defaults() -> Candidate {
        Candidate {
            post: PostSummary::default(),
            author_id: String::new(),
            status: 0,
            quality_score: 0.0,
            recall_score: 0.0,
            score: 0.0,
            p_ctr: 0.0,
            p_cvr: 0.0,
            p_wegu: 0.0,
            feature_snapshot: HashMap::new(),
            source: String::new(),
            reasons: Vec::new(),
            followed_author: false,
            blocked_author: false,
            muted_author: false,
            liked: false,
            bookmarked: false,
            hidden: false,
            previously_served: false,
            daily_served_count: 0,
        }
    }
}
