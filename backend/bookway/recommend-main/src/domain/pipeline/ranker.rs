use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;
use tonic::transport::Channel;

use super::{Candidate, CandidateRanker, FeedQuery, PipelineError, RankOutcome};
use crate::datasource::ModelClientError;
use bookway_feature_main::api::pb::{self as feature, feature_main_client::FeatureMainClient};
use bookway_recommend_rank::api::pb as rank;

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
    ) -> Result<rank::RankResponse, ModelClientError> {
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
            .map_err(|error| ModelClientError::Grpc(error.to_string()))?
            .into_inner();
        let features: FeatureResponse = serde_json::from_str(&response.response_json)
            .map_err(|error| ModelClientError::Grpc(error.to_string()))?;
        let features = features.features;
        let mut client = (*self.ranker).clone();
        let response = client
            .rank(rank::RankRequest {
                user_id: user_id.to_string(),
                features_json: features.to_string(),
                candidates: candidates.iter().map(candidate_to_proto).collect(),
            })
            .await
            .map_err(|status| ModelClientError::Grpc(status.to_string()))?
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

#[derive(Debug, Deserialize)]
struct FeatureResponse {
    features: serde_json::Value,
}

fn candidate_to_proto(candidate: &Candidate) -> rank::Candidate {
    rank::Candidate {
        content_id: candidate.post.id.clone(),
        post_json: serde_json::to_string(&candidate.post).unwrap_or_default(),
        author_id: candidate.author_id.clone(),
        status: serde_json::to_string(&candidate.status).unwrap_or_default(),
        quality_score: candidate.quality_score,
        freshness: candidate.post.freshness,
        recall_score: candidate.score,
        score: candidate.score,
        source: candidate.source.clone(),
        reasons: candidate.reasons.clone(),
    }
}
