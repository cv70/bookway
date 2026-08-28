use bookway_ad_center_api::pb::{self as center, ad_center_client::AdCenterClient};
use bookway_ad_rank_api::pb::{self as rank, ad_rank_client::AdRankClient};
use bookway_ad_recall_api::pb::{self as recall, ad_recall_client::AdRecallClient};

#[path = "pacing.rs"]
mod pacing;
pub(crate) use pacing::ImpressionPacing;

use crate::Config;
use thiserror::Error;
use uuid::Uuid;

const MAX_CONTEXT_FIELD_LENGTH: usize = 160;
const MAX_PLACEMENT_LENGTH: usize = 80;
const MAX_IDENTIFIER_LENGTH: usize = 160;
#[derive(Debug, Error)]
pub(crate) enum AdMainError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Upstream(String),
}
#[derive(Clone)]
pub struct Domain {
    config: Config,
    recall: AdRecallClient<tonic::transport::Channel>,
    rank: AdRankClient<tonic::transport::Channel>,
    center: AdCenterClient<tonic::transport::Channel>,
    pacing: Option<ImpressionPacing>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_runtime::ConnectFailure> {
        let recall = AdRecallClient::new(bookway_runtime::grpc_channel(&config.recall_url).await?);
        let rank = AdRankClient::new(bookway_runtime::grpc_channel(&config.rank_url).await?);
        let center = AdCenterClient::new(bookway_runtime::grpc_channel(&config.center_url).await?);
        // Operator opt-in only: absent config or Redis the decisions flow at
        // their natural cadence.
        let pacing = ImpressionPacing::connect(config.impression_cooldown).await;
        Ok(Self {
            config,
            recall,
            rank,
            center,
            pacing,
        })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn decide(
        &self,
        mut request: crate::api::pb::DecisionRequest,
    ) -> Result<crate::api::pb::DecisionResponse, AdMainError> {
        request.user_id = request.user_id.trim().to_string();
        request.placement = request.placement.trim().to_string();
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        let scene_equipment =
            scene_equipment_key(request.scene_equipment.as_deref().unwrap_or_default());
        // Delivery context travels with the decision so geo/device-targeted
        // campaigns only compete where they were scoped to serve. Absent
        // context stays empty and matches unrestricted campaigns only.
        let geo_region = delivery_context_key(&request.geo_region);
        let device_os = delivery_context_key(&request.device_os);
        if request.user_id.trim().is_empty()
            || request.user_id.chars().count() > MAX_IDENTIFIER_LENGTH
            || request.placement.trim().is_empty()
            || request.placement.trim().chars().count() > MAX_PLACEMENT_LENGTH
            || request.route_id.trim().is_empty()
            || request.route_id.trim().chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.action_node_id.trim().is_empty()
            || request.action_node_id.trim().chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || scene_equipment.is_empty()
            || scene_equipment.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || geo_region.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || device_os.chars().count() > MAX_CONTEXT_FIELD_LENGTH
        {
            return Err(AdMainError::Validation(
                "user_id, placement, route_id, action_node_id and scene_equipment are required"
                    .to_string(),
            ));
        }
        let limit = request.limit.unwrap_or(1).clamp(
            1,
            u32::try_from(self.config.max_decisions.clamp(1, 10)).unwrap_or(10),
        ) as usize;
        // Serving-experience throttle (fail-open): skip ads while a previous
        // decision is still inside the operator's cooldown window. Authoritative
        // frequency limits remain in ad-center receipts.
        if let Some(pacing) = &self.pacing
            && pacing.cooling_down(&request.user_id).await
        {
            return Ok(crate::api::pb::DecisionResponse {
                request_id: Uuid::now_v7().to_string(),
                items: Vec::new(),
                degraded: false,
            });
        }
        let mut recall_client = self.recall.clone();
        let candidates = recall_client
            .recall(service_request(
                "ad-recall",
                recall::RecallRequest {
                    user_id: request.user_id.clone(),
                    placement: request.placement.clone(),
                    domain: request.domain.clone().unwrap_or_default(),
                    limit: u32::try_from(limit.saturating_mul(4)).unwrap_or(u32::MAX),
                    route_id: request.route_id.clone(),
                    action_node_id: request.action_node_id.clone(),
                    scene_equipment: scene_equipment.clone(),
                    geo_region: geo_region.clone(),
                    device_os: device_os.clone(),
                },
            )?)
            .await
            .map_err(|error| upstream_error("ad-recall", error))?
            .into_inner();
        let mut rank_client = self.rank.clone();
        let ranked = rank_client
            .rank(service_request(
                "ad-rank",
                rank::RankRequest {
                    user_id: request.user_id.clone(),
                    domain: request.domain.clone().unwrap_or_default(),
                    candidates: candidates.items,
                    route_id: request.route_id.clone(),
                    action_node_id: request.action_node_id.clone(),
                    scene_equipment: scene_equipment.clone(),
                },
            )?)
            .await
            .map_err(|error| upstream_error("ad-rank", error))?
            .into_inner();
        let request_id = Uuid::now_v7().to_string();
        let items = ranked
            .items
            .into_iter()
            .filter_map(|item| {
                let ecpm = item.ecpm;
                let Some(campaign) = item.campaign else {
                    tracing::warn!("ad-rank returned a candidate without a campaign");
                    return None;
                };
                let context_matches = campaign.placement == request.placement
                    && campaign.route_id == request.route_id
                    && campaign.action_node_id == request.action_node_id
                    && scene_equipment_key(&campaign.scene_equipment) == scene_equipment;
                if !context_matches || campaign.id.trim().is_empty() {
                    tracing::warn!(campaign_id = %campaign.id, "ad-rank returned a candidate outside the requested context");
                    return None;
                }
                if !ecpm.is_finite() || ecpm < 0.0 || !item.score.is_finite() {
                    tracing::warn!(ecpm, score = item.score, "ad-rank returned an invalid auction value; dropping candidate");
                    return None;
                }
                Some(crate::api::pb::AdDecision {
                    request_id: request_id.clone(),
                    campaign_id: campaign.id,
                    placement: campaign.placement,
                    title: campaign.title,
                    body: campaign.body,
                    image_url: campaign.image_url,
                    landing_url: campaign.landing_url,
                    score: item.score,
                    model_version: ranked.model_version.clone(),
                    route_id: campaign.route_id,
                    action_node_id: campaign.action_node_id,
                    scene_equipment: campaign.scene_equipment,
                    ecpm,
                })
            })
            .take(limit)
            .collect::<Vec<_>>();
        if !items.is_empty() {
            let mut center_client = self.center.clone();
            center_client
                .register_decisions(service_request(
                    "ad-center",
                    center::RegisterDecisionRequest {
                        user_id: request.user_id.clone(),
                        request_id: request_id.clone(),
                        placement: request.placement.clone(),
                        campaign_ids: items.iter().map(|item| item.campaign_id.clone()).collect(),
                        route_id: request.route_id.clone(),
                        action_node_id: request.action_node_id.clone(),
                        scene_equipment,
                    },
                )?)
                .await
                .map_err(|error| upstream_error("ad-center", error))?;
        }
        // Arm the window only for decisions that actually carried ads.
        if !items.is_empty() && let Some(pacing) = &self.pacing {
            pacing.mark_served(&request.user_id).await;
        }
        Ok(crate::api::pb::DecisionResponse {
            request_id,
            items,
            degraded: ranked.degraded,
        })
    }
    pub(crate) async fn report_event(
        &self,
        mut request: center::RecordEventRequest,
    ) -> Result<center::EventReceipt, AdMainError> {
        request.user_id = request.user_id.trim().to_string();
        request.event_id = request.event_id.trim().to_string();
        request.request_id = request.request_id.trim().to_string();
        request.campaign_id = request.campaign_id.trim().to_string();
        if invalid_identifier(&request.user_id)
            || invalid_identifier(&request.event_id)
            || invalid_identifier(&request.request_id)
            || invalid_identifier(&request.campaign_id)
        {
            return Err(AdMainError::Validation(
                "user_id, event_id, request_id and campaign_id are required".to_string(),
            ));
        }
        // Conversions are a MONEY fact: the only legitimate producer is the
        // payment pipeline (purchase_event_outbox -> outbox-relay ->
        // ad-center), which carries the server-verified paid order. A client
        // asserting its own purchase would poison pCVR calibration, so the
        // public beacon path admits impressions and clicks only.
        reject_client_asserted_conversion(&request)?;
        let mut client = self.center.clone();
        client
            .record_event(service_request("ad-center", request)?)
            .await
            .map(|response| response.into_inner())
            .map_err(|error| upstream_error("ad-center", error))
    }
}

/// Conversions are a server-side fact: only the payment pipeline
/// (purchase_event_outbox -> cmd/outbox-relay -> ad-center) may report one,
/// because it alone carries a verified paid order. A client-asserted
/// conversion would poison pCVR calibration.
fn reject_client_asserted_conversion(
    request: &center::RecordEventRequest,
) -> Result<(), AdMainError> {
    if request.event_type == center::EventType::Conversion as i32 {
        return Err(AdMainError::Validation(
            "conversions are server-attributed from payment facts; clients may report impressions and clicks only".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod conversion_gate_tests {
    use super::reject_client_asserted_conversion;
    use bookway_ad_center_api::pb as center;

    #[test]
    fn client_conversion_beacons_are_rejected() {
        let request = center::RecordEventRequest {
            user_id: "user-1".to_string(),
            event_id: "event-1".to_string(),
            request_id: "ad-request-1".to_string(),
            campaign_id: "campaign-1".to_string(),
            event_type: center::EventType::Conversion as i32,
        };
        assert!(reject_client_asserted_conversion(&request).is_err());
    }

    #[test]
    fn impression_and_click_beacons_still_pass_the_gate() {
        for event_type in [
            center::EventType::Impression as i32,
            center::EventType::Click as i32,
        ] {
            let request = center::RecordEventRequest {
                user_id: "user-1".to_string(),
                event_id: "event-1".to_string(),
                request_id: "ad-request-1".to_string(),
                campaign_id: "campaign-1".to_string(),
                event_type,
            };
            assert!(reject_client_asserted_conversion(&request).is_ok());
        }
    }
}

fn scene_equipment_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn delivery_context_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn invalid_identifier(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.chars().count() > MAX_IDENTIFIER_LENGTH
}
fn service_request<T>(service: &'static str, value: T) -> Result<tonic::Request<T>, AdMainError> {
    bookway_runtime::grpc_service_request(value)
        .map_err(|error| AdMainError::Upstream(format!("{service} request failed: {error}")))
}
fn upstream_error(service: &'static str, error: tonic::Status) -> AdMainError {
    AdMainError::Upstream(format!("{service} request failed: {error}"))
}
