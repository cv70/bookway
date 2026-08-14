use async_trait::async_trait;

use super::{Candidate, CandidateHydrator, FeedQuery, PipelineError};
use crate::datasource::{
    SharedBbsContextDataSource, SharedExposureDataSource, SharedLikeStatusDataSource,
};

const SERVED_HISTORY_LIMIT: usize = 500;

pub(crate) struct ServedHistoryHydrator {
    exposures: SharedExposureDataSource,
}

impl ServedHistoryHydrator {
    pub(crate) fn new(exposures: SharedExposureDataSource) -> Self {
        Self { exposures }
    }
}

#[async_trait]
impl CandidateHydrator for ServedHistoryHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let served = self
            .exposures
            .recent_content_ids(&query.user_id, SERVED_HISTORY_LIMIT)
            .await;
        for candidate in candidates {
            candidate.previously_served = served.contains(&candidate.post.id);
        }
        Ok(())
    }
}

pub(crate) struct SocialContextHydrator {
    bbs: SharedBbsContextDataSource,
}

impl SocialContextHydrator {
    pub(crate) fn new(bbs: SharedBbsContextDataSource) -> Self {
        Self { bbs }
    }
}

#[async_trait]
impl CandidateHydrator for SocialContextHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let context = self.bbs.context(&query.user_id).await?;
        for candidate in candidates {
            candidate.followed_author = context.followed_author_ids.contains(&candidate.author_id);
            candidate.blocked_author = context.blocked_author_ids.contains(&candidate.author_id);
            candidate.muted_author = context.muted_author_ids.contains(&candidate.author_id);
        }
        Ok(())
    }
}

pub(crate) struct ReactionContextHydrator {
    like_status: SharedLikeStatusDataSource,
}

impl ReactionContextHydrator {
    pub(crate) fn new(like_status: SharedLikeStatusDataSource) -> Self {
        Self { like_status }
    }
}

#[async_trait]
impl CandidateHydrator for ReactionContextHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let post_ids = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        let context = self.like_status.context(&query.user_id, post_ids).await?;
        for candidate in candidates {
            candidate.liked = context.liked_post_ids.contains(&candidate.post.id);
            candidate.bookmarked = context.bookmarked_post_ids.contains(&candidate.post.id);
        }
        Ok(())
    }
}

pub(crate) struct SocialProofHydrator;

#[async_trait]
impl CandidateHydrator for SocialProofHydrator {
    async fn hydrate(
        &self,
        _query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        for candidate in candidates {
            candidate
                .reasons
                .push(format!("{} 人正在同行", candidate.post.join_count));
            if candidate.followed_author {
                candidate.reasons.insert(0, "来自你关注的作者".to_string());
            }
            if candidate.liked {
                candidate.reasons.push("你已经赞过这篇内容".to_string());
            }
            if candidate.bookmarked {
                candidate.reasons.push("你已经收藏过这篇内容".to_string());
            }
        }
        Ok(())
    }
}
