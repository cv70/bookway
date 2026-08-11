use std::collections::HashMap;

use async_trait::async_trait;

use super::{Candidate, CandidateRanker, FeedQuery, PipelineError};
use crate::internal::datasource::SharedModelDataSource;

pub(crate) struct RemoteModelRanker {
    models: SharedModelDataSource,
}
impl RemoteModelRanker {
    pub(crate) fn new(models: SharedModelDataSource) -> Self {
        Self { models }
    }
}

#[async_trait]
impl CandidateRanker for RemoteModelRanker {
    async fn rank(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let request = candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.post.id.clone(),
                    candidate.score,
                    candidate.quality_score,
                    candidate.post.freshness,
                )
            })
            .collect();
        let scores = self
            .models
            .rank(&query.user_id, request)
            .await?
            .into_iter()
            .map(|item| (item.content_id, item.score))
            .collect::<HashMap<_, _>>();
        for candidate in candidates {
            if let Some(score) = scores.get(&candidate.post.id) {
                candidate.score = *score;
                candidate.reasons.push("模型排序".to_string());
            }
        }
        Ok(())
    }
}
