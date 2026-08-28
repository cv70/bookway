use async_trait::async_trait;
use bookway_bbs_api::pb::{self as bbs_pb, bbs_client::BbsClient};
use bookway_interaction_status_api::pb::{
    self as like_pb, interaction_status_client::InteractionStatusClient,
};
use tonic::transport::Channel;

use std::sync::Arc;

use super::{Candidate, CandidateHydrator, FeedQuery, HydratorFailurePolicy, PipelineError};
use crate::datasource::{FrequencyCapDataSource, SharedExposureDataSource};

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
        // Anonymous serving has no ledger identity — nothing was ever
        // recorded for it, so there is nothing to look up.
        let Some(user_id) = query.user_id.as_deref() else {
            return Ok(());
        };
        let served = self
            .exposures
            .recent_content_ids(user_id, &query.surface, SERVED_HISTORY_LIMIT)
            .await;
        for candidate in candidates {
            candidate.previously_served = served.contains(&candidate.post.id);
        }
        Ok(())
    }
}

/// Loads today's served counters in one batch so the frequency filter can run
/// synchronously afterwards. Deliberately best-effort: a failed counter lookup
/// fails OPEN (items stay eligible) because skipping hydration entirely is
/// better UX than an empty feed — the exposure write side still records truth.
pub(crate) struct FrequencyCapHydrator {
    caps: Arc<dyn FrequencyCapDataSource>,
}

impl FrequencyCapHydrator {
    pub(crate) fn new(caps: Arc<dyn FrequencyCapDataSource>) -> Self {
        Self { caps }
    }
}

#[async_trait]
impl CandidateHydrator for FrequencyCapHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        if candidates.is_empty() {
            return Ok(());
        }
        // The guard is identity-scoped; anonymous requests accrue no cap
        // state and therefore none to read.
        let Some(user_id) = query.user_id.as_deref() else {
            return Ok(());
        };
        let content_ids = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect::<Vec<_>>();
        let counts = self.caps.served_counts(user_id, &content_ids).await?;
        for (candidate, count) in candidates.iter_mut().zip(counts) {
            candidate.daily_served_count = count;
        }
        Ok(())
    }
}

pub(crate) struct SocialContextHydrator {
    client: BbsClient<Channel>,
}

impl SocialContextHydrator {
    pub(crate) fn new(client: BbsClient<Channel>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CandidateHydrator for SocialContextHydrator {
    fn failure_policy(&self) -> HydratorFailurePolicy {
        HydratorFailurePolicy::FailClosed
    }

    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let mut context_client = self.client.clone();
        let mut visibility_client = self.client.clone();
        let context_request = bbs_request(bbs_pb::ContextRequest {
            user_id: query.user_id_or_empty().to_string(),
            post_ids: Vec::new(),
        })?;
        let visibility_request = bbs_request(bbs_pb::ContextRequest {
            user_id: query.user_id_or_empty().to_string(),
            post_ids: Vec::new(),
        })?;
        let (context, visibility) = tokio::try_join!(
            context_client.context(context_request),
            visibility_client.visibility_context(visibility_request),
        )
        .map_err(|error| PipelineError::Bbs(error.to_string()))?;
        let context = context.into_inner();
        let visibility = visibility.into_inner();
        for candidate in candidates {
            candidate.followed_author = context.followed_author_ids.contains(&candidate.author_id);
            candidate.blocked_author = visibility
                .excluded_author_ids
                .contains(&candidate.author_id);
            candidate.muted_author = context.muted_author_ids.contains(&candidate.author_id);
        }
        Ok(())
    }
}

pub(crate) struct RouteContextHydrator {
    client: BbsClient<Channel>,
}

impl RouteContextHydrator {
    pub(crate) fn new(client: BbsClient<Channel>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CandidateHydrator for RouteContextHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let route_ids = candidates
            .iter()
            .filter(|candidate| candidate.post.is_route)
            .map(|candidate| candidate.post.id.clone())
            .collect::<Vec<_>>();
        if route_ids.is_empty() {
            return Ok(());
        }
        let mut client = self.client.clone();
        let context = client
            .route_context(bbs_request(bbs_pb::RouteContextRequest {
                user_id: query.user_id_or_empty().to_string(),
                route_ids,
            })?)
            .await
            .map_err(|error| PipelineError::Bbs(error.to_string()))?
            .into_inner();
        for candidate in candidates {
            let live_count = context
                .participant_counts
                .get(&candidate.post.id)
                .copied()
                .unwrap_or_default();
            candidate.post.join_count = candidate
                .post
                .join_count
                .saturating_add(u32::try_from(live_count).unwrap_or(u32::MAX));
            if context.joined_route_ids.contains(&candidate.post.id) {
                candidate.reasons.insert(0, "你正在走这条路线".to_string());
            }
        }
        Ok(())
    }
}

pub(crate) struct ReactionContextHydrator {
    client: InteractionStatusClient<Channel>,
}

impl ReactionContextHydrator {
    pub(crate) fn new(client: InteractionStatusClient<Channel>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CandidateHydrator for ReactionContextHydrator {
    fn failure_policy(&self) -> HydratorFailurePolicy {
        HydratorFailurePolicy::FailClosed
    }

    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let post_ids = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        let mut client = self.client.clone();
        let context = client
            .context(like_pb::ContextRequest {
                user_id: query.user_id.clone(),
                post_ids,
            })
            .await
            .map_err(|error| PipelineError::InteractionStatus(error.to_string()))?
            .into_inner();
        for candidate in candidates {
            candidate.liked = context.liked_post_ids.contains(&candidate.post.id);
            candidate.bookmarked = context.bookmarked_post_ids.contains(&candidate.post.id);
            candidate.hidden = context.hidden_post_ids.contains(&candidate.post.id);
        }
        Ok(())
    }
}

fn bbs_request<T>(message: T) -> Result<tonic::Request<T>, PipelineError> {
    bookway_runtime::grpc_service_request(message)
        .map_err(|error| PipelineError::Bbs(error.to_string()))
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
            if candidate.post.is_milestone {
                candidate
                    .reasons
                    .insert(0, "来自公开路线的阶段成果".to_string());
            }
            if candidate.post.is_route && candidate.post.join_count > 0 {
                candidate
                    .reasons
                    .push(format!("{} 人正在同行", candidate.post.join_count));
            }
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
