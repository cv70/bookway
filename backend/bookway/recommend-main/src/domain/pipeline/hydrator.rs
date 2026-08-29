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
    fn merge(&self, target: &mut [Candidate], hydrated: &[Candidate]) {
        for (target, hydrated) in target.iter_mut().zip(hydrated) {
            target.previously_served = hydrated.previously_served;
        }
    }

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
    fn merge(&self, target: &mut [Candidate], hydrated: &[Candidate]) {
        for (target, hydrated) in target.iter_mut().zip(hydrated) {
            target.daily_served_count = hydrated.daily_served_count;
        }
    }

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

    fn merge(&self, target: &mut [Candidate], hydrated: &[Candidate]) {
        for (target, hydrated) in target.iter_mut().zip(hydrated) {
            target.followed_author = hydrated.followed_author;
            target.blocked_author = hydrated.blocked_author;
            target.muted_author = hydrated.muted_author;
        }
    }

    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        // Follow, block and mute are identity-scoped edges. BBS rejects an
        // empty user id outright, so calling this anonymously would fail closed
        // and blank the whole logged-out feed. Anonymous serving simply has no
        // social graph to read.
        let Some(user_id) = query.user_id.as_deref() else {
            return Ok(());
        };
        let mut context_client = self.client.clone();
        let mut visibility_client = self.client.clone();
        let context_request = bbs_request(bbs_pb::ContextRequest {
            user_id: user_id.to_string(),
            post_ids: Vec::new(),
        })?;
        let visibility_request = bbs_request(bbs_pb::ContextRequest {
            user_id: user_id.to_string(),
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
    fn merge(&self, target: &mut [Candidate], hydrated: &[Candidate]) {
        for (target, hydrated) in target.iter_mut().zip(hydrated) {
            target.post.join_count = hydrated.post.join_count;
            target.reasons = hydrated.reasons.clone();
        }
    }

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
            // BBS owns participation. The count is assigned from the fact just
            // read; a candidate BBS did not answer for stays absent rather than
            // inheriting a number from the candidate source.
            candidate.post.join_count = context
                .participant_counts
                .get(&candidate.post.id)
                .copied()
                .map(|count| u32::try_from(count).unwrap_or(u32::MAX));
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

    fn merge(&self, target: &mut [Candidate], hydrated: &[Candidate]) {
        for (target, hydrated) in target.iter_mut().zip(hydrated) {
            target.liked = hydrated.liked;
            target.bookmarked = hydrated.bookmarked;
            target.hidden = hydrated.hidden;
        }
    }

    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        // Reactions are identity-scoped. Reading them anonymously would have
        // to invent an identity, and every anonymous visitor would then share
        // one reaction bucket.
        if query.user_id.is_none() {
            return Ok(());
        }
        let post_ids = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        let mut client = self.client.clone();
        let context = client
            .context(interaction_status_request(like_pb::ContextRequest {
                user_id: query.user_id.clone(),
                post_ids,
            })?)
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

/// Interaction-status sits behind `grpc_service_auth_interceptor` like every
/// other business service, so its requests must carry the service token too.
fn interaction_status_request<T>(message: T) -> Result<tonic::Request<T>, PipelineError> {
    bookway_runtime::grpc_service_request(message)
        .map_err(|error| PipelineError::InteractionStatus(error.to_string()))
}

pub(crate) struct SocialProofHydrator;

#[async_trait]
impl CandidateHydrator for SocialProofHydrator {
    fn depends_on_previous(&self) -> bool {
        true
    }

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
            // Only claim companionship when the live fact was actually read.
            if let Some(join_count) = candidate
                .post
                .join_count
                .filter(|count| *count > 0 && candidate.post.is_route)
            {
                candidate.reasons.push(format!("{join_count} 人正在同行"));
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        BbsClient, CandidateHydrator, Channel, FeedQuery, InteractionStatusClient,
        ReactionContextHydrator, SocialContextHydrator,
    };

    /// A lazily-connected channel to a port nothing listens on. It never dials
    /// until an RPC is actually issued, so a hydrator that correctly skips the
    /// call returns `Ok`, while one that issues it fails on transport.
    fn undialed_channel() -> Channel {
        tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy()
    }

    fn anonymous_query() -> FeedQuery {
        FeedQuery {
            interests: HashSet::new(),
            seen: HashSet::new(),
            user_id: None,
            session_id: Some("session-1".to_string()),
            surface: "home".to_string(),
            cursor: None,
            limit: 10,
            geo_region: String::new(),
            device_os: String::new(),
        }
    }

    /// Both of these hydrators are FailClosed, so an error from them blanks the
    /// entire page. Anonymous serving has no social graph and no reactions, and
    /// the upstreams reject an empty identity — so they must skip the call
    /// rather than fail closed on every logged-out request.
    #[tokio::test]
    async fn identity_scoped_hydrators_skip_anonymous_requests() {
        let query = anonymous_query();

        SocialContextHydrator::new(BbsClient::new(undialed_channel()))
            .hydrate(&query, &mut [])
            .await
            .expect("anonymous social hydration must not fail closed");

        ReactionContextHydrator::new(InteractionStatusClient::new(undialed_channel()))
            .hydrate(&query, &mut [])
            .await
            .expect("anonymous reaction hydration must not fail closed");
    }
}
