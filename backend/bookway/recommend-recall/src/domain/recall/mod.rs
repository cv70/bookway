mod candidate;

use std::collections::HashSet;

use futures::future::join_all;

use crate::api::pb;
use crate::domain::Domain;

impl Domain {
    pub(crate) async fn recall(&self, request: pb::RecallRequest) -> pb::RecallResponse {
        let limit = (request.limit as usize).clamp(1, self.max_candidates);
        let cursor = (!request.cursor.is_empty()).then_some(request.cursor);
        let seen = candidate::seen(&request.seen);
        let jobs = ["quality", "fresh"].into_iter().map(|strategy| {
            let content = self.content.clone();
            let cursor = cursor.clone();
            async move {
                (
                    strategy,
                    content
                        .list(strategy, cursor, (limit * 3).min(self.max_candidates))
                        .await,
                )
            }
        });
        let mut candidates = Vec::new();
        let mut next_cursor = None;
        let mut degraded = false;
        for (strategy, result) in join_all(jobs).await {
            match result {
                Ok(page) => {
                    if next_cursor.is_none() {
                        next_cursor = page.next_cursor;
                    }
                    candidates.extend(
                        page.items
                            .into_iter()
                            .map(|content| candidate::candidate_from_content(content, strategy)),
                    );
                }
                Err(error) => {
                    degraded = true;
                    tracing::warn!(%error, strategy, "recall source degraded");
                }
            }
        }
        let mut ids = HashSet::new();
        candidates.retain(|candidate| {
            !seen.contains(&candidate.content_id) && ids.insert(candidate.content_id.clone())
        });
        candidates.sort_by(|left, right| {
            right
                .recall_score
                .total_cmp(&left.recall_score)
                .then_with(|| left.content_id.cmp(&right.content_id))
        });
        candidates.truncate(limit);
        pb::RecallResponse {
            candidates: candidates.into_iter().map(candidate::to_proto).collect(),
            next_cursor: next_cursor.unwrap_or_default(),
            sources: vec!["recall:quality".to_string(), "recall:fresh".to_string()],
            degraded,
        }
    }
}
