#![allow(clippy::module_inception)]

mod domain;
pub(crate) mod pipeline;

use self::pipeline::FeedPipeline;
use super::api::pb;
use bookway_ad_main_api::pb::{self as ad_pb, ad_main_client::AdMainClient};

#[derive(Clone)]
pub(crate) struct FeedService {
    pipeline: FeedPipeline,
    ad_main: AdMainClient<tonic::transport::Channel>,
}

pub use domain::Domain;

impl FeedService {
    pub(crate) fn new(
        pipeline: FeedPipeline,
        ad_main: AdMainClient<tonic::transport::Channel>,
    ) -> Self {
        Self { pipeline, ad_main }
    }

    pub(crate) async fn recommend(&self, request: pb::FeedRequest) -> pb::FeedResponse {
        let action_context = request.action_context.clone();
        let limit = request.limit.unwrap_or(20).clamp(1, 100) as usize;
        let mut response = self.pipeline.execute(request.clone()).await;
        let Some(context) = action_context.filter(valid_action_context) else {
            return response;
        };
        // Keep an ad below three organic recommendations. This both preserves
        // the action feed's utility and avoids registering an unrenderable ad
        // on very small pages.
        if response.items.len() < 3 || limit < 4 {
            return response;
        }

        let ad_request = match service_request(ad_pb::DecisionRequest {
            user_id: request.user_id,
            placement: context.placement.clone(),
            domain: context.domain.clone(),
            limit: Some(1),
            route_id: context.route_id.clone(),
            action_node_id: context.action_node_id.clone(),
            scene_equipment: context.scene_equipment.clone(),
        }) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, "contextual ad request setup degraded; serving organic feed");
                if let Some(meta) = &mut response.meta {
                    meta.degraded = true;
                }
                return response;
            }
        };
        let mut ad_main = self.ad_main.clone();
        match ad_main.decide(ad_request).await {
            Ok(decision) => {
                mix_contextual_ad(&mut response, decision.into_inner(), &context, limit)
            }
            Err(error) => {
                tracing::warn!(%error, "contextual ad decision degraded; serving organic feed");
                if let Some(meta) = &mut response.meta {
                    meta.degraded = true;
                }
            }
        }
        response
    }
}

fn valid_action_context(context: &pb::FeedActionContext) -> bool {
    !context.route_id.trim().is_empty()
        && !context.action_node_id.trim().is_empty()
        && !context.placement.trim().is_empty()
        && context
            .scene_equipment
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn mix_contextual_ad(
    response: &mut pb::FeedResponse,
    decision: ad_pb::DecisionResponse,
    context: &pb::FeedActionContext,
    limit: usize,
) {
    let ad_pb::DecisionResponse {
        items, degraded, ..
    } = decision;
    let Some(ad) = items.into_iter().find(|ad| {
        ad.route_id == context.route_id
            && ad.action_node_id == context.action_node_id
            && ad.placement == context.placement
            && ad.scene_equipment == context.scene_equipment.clone().unwrap_or_default()
    }) else {
        if degraded {
            if let Some(meta) = &mut response.meta {
                meta.degraded = true;
            }
        }
        return;
    };
    let insertion_index = response.items.len().min(3);
    response.items.insert(
        insertion_index,
        pb::FeedItem {
            author_id: String::new(),
            post: None,
            score: ad.score,
            source: "contextual_ad_ecpm".to_string(),
            reasons: vec![
                "action_node_context".to_string(),
                "ecpm_auction".to_string(),
            ],
            ad: Some(pb::FeedAd {
                request_id: ad.request_id,
                campaign_id: ad.campaign_id,
                placement: ad.placement,
                title: ad.title,
                body: ad.body,
                image_url: ad.image_url,
                landing_url: ad.landing_url,
                ecpm: ad.score,
                model_version: ad.model_version,
                route_id: ad.route_id,
                action_node_id: ad.action_node_id,
                scene_equipment: ad.scene_equipment,
            }),
        },
    );
    response.items.truncate(limit);
    if degraded {
        if let Some(meta) = &mut response.meta {
            meta.degraded = true;
        }
    }
}

fn service_request<T>(
    value: T,
) -> Result<tonic::Request<T>, bookway_runtime::GrpcServiceAuthError> {
    bookway_runtime::grpc_service_request(value)
}

#[cfg(test)]
mod tests {
    use super::{mix_contextual_ad, valid_action_context};
    use crate::api::pb;
    use bookway_ad_main_api::pb as ad_pb;

    fn context() -> pb::FeedActionContext {
        pb::FeedActionContext {
            route_id: "route-1".to_string(),
            action_node_id: "node-1".to_string(),
            placement: "route_feed".to_string(),
            domain: Some("movement".to_string()),
            scene_equipment: Some("trail shoes".to_string()),
        }
    }

    fn organic(id: &str) -> pb::FeedItem {
        pb::FeedItem {
            author_id: id.to_string(),
            post: None,
            score: 0.8,
            source: "organic".to_string(),
            reasons: Vec::new(),
            ad: None,
        }
    }

    #[test]
    fn contextual_ad_is_low_density_and_bound_to_its_action_node() {
        let mut response = pb::FeedResponse {
            request_id: "feed-1".to_string(),
            items: (0..5).map(|index| organic(&index.to_string())).collect(),
            meta: Some(pb::FeedMeta {
                sourced: 5,
                filtered: 0,
                selected: 5,
                next_cursor: None,
                pipeline_id: "test".to_string(),
                degraded: false,
                model_version: None,
                experiment_bucket: None,
            }),
        };
        mix_contextual_ad(
            &mut response,
            ad_pb::DecisionResponse {
                request_id: "ad-request-1".to_string(),
                items: vec![ad_pb::AdDecision {
                    request_id: "ad-request-1".to_string(),
                    campaign_id: "campaign-1".to_string(),
                    placement: "route_feed".to_string(),
                    title: "Trail shoes".to_string(),
                    body: String::new(),
                    image_url: String::new(),
                    landing_url: String::new(),
                    score: 42.0,
                    model_version: "ecpm-v1".to_string(),
                    route_id: "route-1".to_string(),
                    action_node_id: "node-1".to_string(),
                    scene_equipment: "trail shoes".to_string(),
                    ecpm: 42.0,
                }],
                degraded: false,
            },
            &context(),
            5,
        );

        assert_eq!(response.items.len(), 5);
        assert_eq!(response.items[3].ad.as_ref().map(|ad| ad.ecpm), Some(42.0));
        assert!(
            response
                .items
                .iter()
                .filter(|item| item.ad.is_some())
                .count()
                == 1
        );
    }

    #[test]
    fn incomplete_action_context_never_requests_ads() {
        let mut invalid = context();
        invalid.action_node_id.clear();
        assert!(!valid_action_context(&invalid));
    }

    #[test]
    fn equipment_mismatch_never_enters_the_contextual_slot() {
        let mut response = pb::FeedResponse {
            request_id: "feed-2".to_string(),
            items: (0..4).map(|index| organic(&index.to_string())).collect(),
            meta: Some(pb::FeedMeta {
                sourced: 4,
                filtered: 0,
                selected: 4,
                next_cursor: None,
                pipeline_id: "test".to_string(),
                degraded: false,
                model_version: None,
                experiment_bucket: None,
            }),
        };
        let mut decision = ad_pb::DecisionResponse {
            request_id: "ad-request-2".to_string(),
            items: vec![ad_pb::AdDecision {
                request_id: "ad-request-2".to_string(),
                campaign_id: "campaign-2".to_string(),
                placement: "route_feed".to_string(),
                route_id: "route-1".to_string(),
                action_node_id: "node-1".to_string(),
                scene_equipment: "different equipment".to_string(),
                ..Default::default()
            }],
            degraded: false,
        };
        mix_contextual_ad(&mut response, decision.clone(), &context(), 4);
        assert!(response.items.iter().all(|item| item.ad.is_none()));
        decision.items[0].scene_equipment = "trail shoes".to_string();
        mix_contextual_ad(&mut response, decision, &context(), 4);
        assert_eq!(
            response
                .items
                .iter()
                .filter(|item| item.ad.is_some())
                .count(),
            1
        );
    }
}
