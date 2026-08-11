use async_trait::async_trait;

use super::{Candidate, CandidateSource, FeedQuery, PipelineError, SourceResult};
use crate::internal::datasource::SharedBbsLinkDataSource;

#[derive(Clone, Copy)]
pub(crate) enum SourceStrategy {
    Quality,
    Fresh,
}

impl SourceStrategy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Quality => "quality",
            Self::Fresh => "fresh",
        }
    }
}

pub(crate) struct ContentCandidateSource {
    content: SharedBbsLinkDataSource,
    strategy: SourceStrategy,
}

impl ContentCandidateSource {
    pub(crate) fn new(content: SharedBbsLinkDataSource, strategy: SourceStrategy) -> Self {
        Self { content, strategy }
    }
}

#[async_trait]
impl CandidateSource for ContentCandidateSource {
    async fn get(&self, query: &FeedQuery) -> Result<SourceResult, PipelineError> {
        let page = self
            .content
            .list(
                self.strategy.as_str(),
                query.cursor.clone(),
                query.limit * 3,
            )
            .await?;
        let candidates = page
            .items
            .into_iter()
            .map(|content| Candidate {
                post: content.post,
                author_id: content.author_id,
                status: content.status,
                quality_score: content.quality_score,
                score: 0.0,
                source: match self.strategy {
                    SourceStrategy::Quality => "content:quality".to_string(),
                    SourceStrategy::Fresh => "content:fresh".to_string(),
                },
                reasons: Vec::new(),
                followed_author: false,
                blocked_author: false,
                muted_author: false,
                liked: false,
                bookmarked: false,
            })
            .collect();
        Ok(SourceResult {
            candidates,
            next_cursor: page.next_cursor,
        })
    }
}
