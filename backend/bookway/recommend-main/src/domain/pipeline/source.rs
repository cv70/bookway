use async_trait::async_trait;
use tonic::transport::Channel;

use super::{Candidate, CandidateSource, FeedQuery, PipelineError, SourceResult};
use crate::datasource::RecallClientError;
use bookway_recommend_recall::api::pb as recall;

pub(crate) struct RecommendRecallSource {
    client: std::sync::Arc<recall::recommend_recall_client::RecommendRecallClient<Channel>>,
}

impl RecommendRecallSource {
    pub(crate) fn new(
        client: std::sync::Arc<recall::recommend_recall_client::RecommendRecallClient<Channel>>,
    ) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CandidateSource for RecommendRecallSource {
    async fn get(&self, query: &FeedQuery) -> Result<SourceResult, PipelineError> {
        let mut client = (*self.client).clone();
        let response = client
            .recall(recall::RecallRequest {
                user_id: query.user_id.clone(),
                interests: query
                    .interests
                    .iter()
                    .map(|value| match value {
                        crate::api::GrowthDomainDto::Learning => "learning",
                        crate::api::GrowthDomainDto::Movement => "movement",
                        crate::api::GrowthDomainDto::Wellness => "wellness",
                        crate::api::GrowthDomainDto::Travel => "travel",
                        crate::api::GrowthDomainDto::Leisure => "leisure",
                    })
                    .map(str::to_string)
                    .collect(),
                seen: query.seen.iter().cloned().collect(),
                cursor: query.cursor.clone().unwrap_or_default(),
                limit: (query.limit * 3) as u32,
            })
            .await
            .map_err(|status| PipelineError::Recall(RecallClientError::Grpc(status.to_string())))?
            .into_inner();
        let candidates = response
            .candidates
            .into_iter()
            .filter_map(|candidate| candidate_to_domain(candidate).ok())
            .collect();
        Ok(SourceResult {
            candidates,
            next_cursor: (!response.next_cursor.is_empty()).then_some(response.next_cursor),
        })
    }
}

fn candidate_to_domain(candidate: recall::Candidate) -> Result<Candidate, serde_json::Error> {
    Ok(Candidate {
        post: serde_json::from_str(&candidate.post_json)?,
        author_id: candidate.author_id,
        status: serde_json::from_str(&candidate.status)?,
        quality_score: candidate.quality_score,
        score: candidate.recall_score,
        source: candidate.source,
        reasons: candidate.reasons,
        followed_author: false,
        blocked_author: false,
        muted_author: false,
        liked: false,
        bookmarked: false,
        previously_served: false,
    })
}
