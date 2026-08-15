use std::{collections::BTreeSet, sync::Arc, time::Duration};

use async_trait::async_trait;
use bookway_feature_main::api::pb::{self as feature, feature_main_client::FeatureMainClient};
use serde::Deserialize;
use tonic::transport::Channel;

use super::{Candidate, CandidateSource, FeedQuery, PipelineError, SourceResult};
use crate::datasource::RecallClientError;
use bookway_recommend_recall::api::pb as recall;

const PROFILE_FEATURE_TIMEOUT: Duration = Duration::from_millis(150);
const PERSONALIZED_INTEREST_MINIMUM: f64 = 0.2;

pub(crate) struct RecommendRecallSource {
    client: Arc<recall::recommend_recall_client::RecommendRecallClient<Channel>>,
    feature_client: FeatureMainClient<Channel>,
}

impl RecommendRecallSource {
    pub(crate) fn new(
        client: Arc<recall::recommend_recall_client::RecommendRecallClient<Channel>>,
        feature_client: FeatureMainClient<Channel>,
    ) -> Self {
        Self {
            client,
            feature_client,
        }
    }

    async fn interests(&self, query: &FeedQuery) -> (Vec<String>, bool) {
        let mut interests = query
            .interests
            .iter()
            .map(|domain| domain_name(*domain).to_string())
            .collect::<BTreeSet<_>>();
        let mut client = self.feature_client.clone();
        let profile = tokio::time::timeout(
            PROFILE_FEATURE_TIMEOUT,
            client.features(feature::FeaturesRequest {
                user_id: query.user_id.clone(),
                content_ids: Vec::new(),
            }),
        )
        .await;
        let degraded = match profile {
            Ok(Ok(response)) => {
                match serde_json::from_str::<FeaturePayload>(&response.into_inner().response_json) {
                    Ok(response) => {
                        interests.extend(personalized_interest_domains(&response.features));
                        false
                    }
                    Err(error) => {
                        tracing::warn!(%error, user_id = %query.user_id, "profile feature payload degraded");
                        true
                    }
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, user_id = %query.user_id, "profile feature lookup degraded");
                true
            }
            Err(_) => {
                tracing::warn!(user_id = %query.user_id, "profile feature lookup timed out");
                true
            }
        };
        (interests.into_iter().collect(), degraded)
    }
}

#[async_trait]
impl CandidateSource for RecommendRecallSource {
    async fn get(&self, query: &FeedQuery) -> Result<SourceResult, PipelineError> {
        let (interests, profile_degraded) = self.interests(query).await;
        let mut client = (*self.client).clone();
        let response = client
            .recall(recall::RecallRequest {
                user_id: query.user_id.clone(),
                interests,
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
            degraded: response.degraded || profile_degraded,
        })
    }
}

#[derive(Deserialize)]
struct FeaturePayload {
    features: serde_json::Value,
}

fn personalized_interest_domains(features: &serde_json::Value) -> Vec<String> {
    let mut domains = ["learning", "movement", "wellness", "travel", "leisure"]
        .into_iter()
        .filter_map(|domain| {
            features
                .get(format!("domain_interest.{domain}"))
                .and_then(serde_json::Value::as_f64)
                .filter(|score| score.is_finite() && *score >= PERSONALIZED_INTEREST_MINIMUM)
                .map(|score| (domain, score))
        })
        .collect::<Vec<_>>();
    domains.sort_by(|left, right| right.1.total_cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    domains
        .into_iter()
        .map(|(domain, _)| domain.to_string())
        .collect()
}

fn domain_name(domain: crate::api::GrowthDomainDto) -> &'static str {
    match domain {
        crate::api::GrowthDomainDto::Learning => "learning",
        crate::api::GrowthDomainDto::Movement => "movement",
        crate::api::GrowthDomainDto::Wellness => "wellness",
        crate::api::GrowthDomainDto::Travel => "travel",
        crate::api::GrowthDomainDto::Leisure => "leisure",
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
        hidden: false,
        previously_served: false,
    })
}

#[cfg(test)]
mod tests {
    use super::personalized_interest_domains;

    #[test]
    fn expands_recall_only_for_meaningful_profile_domains() {
        let domains = personalized_interest_domains(&serde_json::json!({
            "domain_interest.wellness": 0.9,
            "domain_interest.leisure": 0.2,
            "domain_interest.travel": 0.19,
        }));

        assert_eq!(domains, ["wellness", "leisure"]);
    }
}
