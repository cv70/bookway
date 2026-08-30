use std::{collections::BTreeSet, sync::Arc, time::Duration};

use async_trait::async_trait;
use bookway_bbs_api::pb::{self as bbs_pb, bbs_client::BbsClient};
use bookway_bbs_link_api::pb::GrowthDomain;
use bookway_feature_main_api::pb::{self as feature, feature_main_client::FeatureMainClient};
use tonic::transport::Channel;

use super::{Candidate, CandidateSource, FeedQuery, PipelineError, SourceResult};
use bookway_recommend_recall_api::pb as recall;

// Profile expansion is optional and runs before the recall RPC. Keep its
// outage budget well below Recommend Main's 140ms request deadline so a cache
// or feature-service failure still reaches the normal recall fallback.
const PROFILE_FEATURE_TIMEOUT: Duration = Duration::from_millis(25);
const RECALL_TIMEOUT: Duration = Duration::from_millis(90);
const FOLLOWING_CONTEXT_TIMEOUT: Duration = Duration::from_millis(35);
const PERSONALIZED_INTEREST_MINIMUM: f64 = 0.2;

pub(crate) struct RecommendRecallSource {
    client: Arc<recall::recommend_recall_client::RecommendRecallClient<Channel>>,
    feature_client: FeatureMainClient<Channel>,
    bbs_client: BbsClient<Channel>,
}

impl RecommendRecallSource {
    pub(crate) fn new(
        client: Arc<recall::recommend_recall_client::RecommendRecallClient<Channel>>,
        feature_client: FeatureMainClient<Channel>,
        bbs_client: BbsClient<Channel>,
    ) -> Self {
        Self {
            client,
            feature_client,
            bbs_client,
        }
    }

    async fn interests(&self, query: &FeedQuery) -> (Vec<GrowthDomain>, bool) {
        let mut interests = query.interests.iter().copied().collect::<BTreeSet<_>>();
        // Anonymous requests have no feature namespace. Calling Feature Main
        // with an empty user id would make every visitor share one cache key
        // and one pointless database lookup, while it cannot add personalized
        // interests anyway.
        if query
            .user_id
            .as_deref()
            .is_none_or(|user_id| user_id.trim().is_empty())
        {
            return (interests.into_iter().collect(), false);
        }
        let mut client = self.feature_client.clone();
        let request = match bookway_runtime::grpc_service_request(feature::FeaturesRequest {
            user_id: query.user_id_or_empty().to_string(),
            content_ids: Vec::new(),
        }) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, user_id = query.user_id_or_empty(), "profile feature request authentication degraded");
                return (interests.into_iter().collect(), true);
            }
        };
        let profile = tokio::time::timeout(PROFILE_FEATURE_TIMEOUT, client.features(request)).await;
        let degraded = match profile {
            Ok(Ok(response)) => {
                interests.extend(personalized_interest_domains(&response.into_inner()));
                false
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, user_id = query.user_id_or_empty(), "profile feature lookup degraded");
                true
            }
            Err(_) => {
                tracing::warn!(user_id = query.user_id_or_empty(), "profile feature lookup timed out");
                true
            }
        };
        (interests.into_iter().collect(), degraded)
    }

    async fn following_author_ids(&self, query: &FeedQuery) -> Result<Vec<String>, PipelineError> {
        if query.surface != "following" {
            return Ok(Vec::new());
        }
        let mut client = self.bbs_client.clone();
        let context = tokio::time::timeout(
            FOLLOWING_CONTEXT_TIMEOUT,
            client.context(
                bookway_runtime::grpc_service_request(bbs_pb::ContextRequest {
                    user_id: query.user_id_or_empty().to_string(),
                    post_ids: Vec::new(),
                })
                .map_err(|error| PipelineError::Bbs(error.to_string()))?,
            ),
        )
        .await
        .map_err(|_| PipelineError::Bbs("following context request timed out".to_string()))?
        .map_err(|error| PipelineError::Bbs(error.to_string()))?
            .into_inner();
        Ok(context.followed_author_ids)
    }
}

#[async_trait]
impl CandidateSource for RecommendRecallSource {
    async fn get(&self, query: &FeedQuery) -> Result<SourceResult, PipelineError> {
        let (interests, profile_degraded) = if query.surface == "following" {
            // A Following feed is a social, chronological product rather than
            // an interest-expansion surface.
            (Vec::new(), false)
        } else {
            self.interests(query).await
        };
        let following_author_ids = self.following_author_ids(query).await?;
        let mut client = (*self.client).clone();
        let response = tokio::time::timeout(
            RECALL_TIMEOUT,
            client.recall(
                bookway_runtime::grpc_service_request(recall::RecallRequest {
                    user_id: query.user_id_or_empty().to_string(),
                    interests: interests.into_iter().map(|domain| domain as i32).collect(),
                    seen: query.seen.iter().cloned().collect(),
                    cursor: query.cursor.clone().unwrap_or_default(),
                    limit: u32::try_from(if query.surface == "following" {
                        query.limit
                    } else {
                        query.limit.saturating_mul(3)
                    })
                    .unwrap_or(u32::MAX),
                    following_author_ids,
                    following_only: query.surface == "following",
                })
                .map_err(|error| PipelineError::Recall(error.to_string()))?,
            ),
        )
        .await
        .map_err(|_| PipelineError::Recall("recommend-recall request timed out".to_string()))?
        .map_err(|status| PipelineError::Recall(status.to_string()))?
        .into_inner();
        let candidates = response
            .candidates
            .into_iter()
            .filter_map(candidate_to_domain)
            .collect();
        Ok(SourceResult {
            candidates,
            next_cursor: (!response.next_cursor.is_empty()).then_some(response.next_cursor),
            degraded: response.degraded || profile_degraded,
            pipeline_version: (!response.blend_version.is_empty())
                .then_some(response.blend_version),
        })
    }
}

fn personalized_interest_domains(features: &feature::FeaturesResponse) -> Vec<GrowthDomain> {
    let mut domains = [
        (GrowthDomain::Learning, features.learning_interest),
        (GrowthDomain::Movement, features.movement_interest),
        (GrowthDomain::Wellness, features.wellness_interest),
        (GrowthDomain::Travel, features.travel_interest),
        (GrowthDomain::Leisure, features.leisure_interest),
    ]
    .into_iter()
    .filter_map(|(domain, score)| {
        (score.is_finite() && score >= PERSONALIZED_INTEREST_MINIMUM).then_some((domain, score))
    })
    .collect::<Vec<_>>();
    domains.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    domains.into_iter().map(|(domain, _)| domain).collect()
}

fn candidate_to_domain(candidate: recall::Candidate) -> Option<Candidate> {
    Some(Candidate {
        post: candidate.post?,
        author_id: candidate.author_id,
        status: candidate.status,
        quality_score: candidate.quality_score,
        recall_score: candidate.recall_score,
        score: candidate.recall_score,
        // Recall sources carry no objective estimates yet; recommend-rank
        // fills them in during ranking.
        p_ctr: 0.0,
        p_cvr: 0.0,
        p_wegu: 0.0,
        // Recall sources carry no model features; recommend-rank fills the
        // snapshot during ranking.
        feature_snapshot: std::collections::HashMap::new(),
        source: candidate.source,
        reasons: candidate.reasons,
        followed_author: false,
        blocked_author: false,
        muted_author: false,
        liked: false,
        bookmarked: false,
        hidden: false,
        previously_served: false,
        daily_served_count: 0,
    })
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb::GrowthDomain;
    use bookway_feature_main_api::pb::FeaturesResponse;
    use std::time::Duration;

    use super::{
        FOLLOWING_CONTEXT_TIMEOUT, PROFILE_FEATURE_TIMEOUT, RECALL_TIMEOUT,
        personalized_interest_domains,
    };

    #[test]
    fn profile_expansion_budget_stays_inside_the_feed_deadline() {
        assert!(PROFILE_FEATURE_TIMEOUT < Duration::from_millis(140));
        assert!(PROFILE_FEATURE_TIMEOUT + RECALL_TIMEOUT < Duration::from_millis(140));
        assert!(FOLLOWING_CONTEXT_TIMEOUT < Duration::from_millis(140));
    }

    #[test]
    fn expands_recall_only_for_meaningful_profile_domains() {
        let domains = personalized_interest_domains(&FeaturesResponse {
            wellness_interest: 0.9,
            leisure_interest: 0.2,
            travel_interest: 0.19,
            ..Default::default()
        });

        assert_eq!(domains, [GrowthDomain::Wellness, GrowthDomain::Leisure]);
    }
}
