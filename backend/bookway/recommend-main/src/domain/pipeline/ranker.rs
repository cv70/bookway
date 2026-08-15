use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tonic::transport::Channel;

use super::{Candidate, CandidateRanker, FeedQuery, PipelineError, RankOutcome};
use bookway_feature_main_api::pb::{self as feature, feature_main_client::FeatureMainClient};
use bookway_recommend_rank_api::pb as rank;
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

    async fn request_scores(
        &self,
        user_id: &str,
        candidates: &[Candidate],
    ) -> Result<rank::RankResponse, PipelineError> {
        let ids: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        let mut feature_client = self.feature_client.clone();
        let response = feature_client
            .features(feature::FeaturesRequest {
                user_id: user_id.to_string(),
                content_ids: ids,
            })
            .await
            .map_err(|error| PipelineError::Model(error.to_string()))?
            .into_inner();
        let mut client = (*self.ranker).clone();
        let response = client
            .rank(rank::RankRequest {
                user_id: user_id.to_string(),
                features: Some(rank_features(response)),
                candidates: candidates.iter().map(candidate_to_proto).collect(),
            })
            .await
            .map_err(|status| PipelineError::Model(status.to_string()))?
            .into_inner();
        Ok(response)
    }
}

#[async_trait]
impl CandidateRanker for RecommendRanker {
    async fn rank(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<RankOutcome, PipelineError> {
        let response = self.request_scores(&query.user_id, candidates).await?;
        let expected_scores = candidates.len();
        let scores = response
            .candidates
            .into_iter()
            .map(|candidate| (candidate.content_id, candidate.score))
            .collect::<HashMap<_, _>>();
        let mut scored_candidates = 0;
        for candidate in candidates {
            if let Some(score) = scores.get(&candidate.post.id) {
                scored_candidates += 1;
                candidate.score = *score;
                candidate.reasons.push("模型排序".to_string());
            }
        }
        Ok(RankOutcome {
            model_version: (!response.model_version.is_empty()).then_some(response.model_version),
            experiment_bucket: (!response.experiment_bucket.is_empty())
                .then_some(response.experiment_bucket),
            degraded: response.degraded || scored_candidates != expected_scores,
        })
    }
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
        recall_score: candidate.score,
        score: candidate.score,
        source: candidate.source.clone(),
        reasons: candidate.reasons.clone(),
    }
}
