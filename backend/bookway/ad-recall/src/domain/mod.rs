use bookway_ad_center_api::pb::{self as center, ad_center_client::AdCenterClient};

use crate::Config;

const MAX_CONTEXT_FIELD_LENGTH: usize = 160;
const MAX_IDENTIFIER_LENGTH: usize = 160;

#[derive(Clone)]
pub struct Domain {
    config: Config,
    center: AdCenterClient<tonic::transport::Channel>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_runtime::ConnectFailure> {
        let center = AdCenterClient::new(bookway_runtime::grpc_channel(&config.center_url).await?);
        Ok(Self { config, center })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn recall(
        &self,
        mut request: crate::api::pb::RecallRequest,
    ) -> Result<center::CampaignList, tonic::Status> {
        request.user_id = request.user_id.trim().to_string();
        request.placement = request.placement.trim().to_string();
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        request.scene_equipment = request.scene_equipment.trim().to_string();
        let geo_region = request.geo_region.trim().to_lowercase();
        let device_os = request.device_os.trim().to_lowercase();
        if request.user_id.trim().is_empty()
            || request.user_id.chars().count() > MAX_IDENTIFIER_LENGTH
            || request.placement.trim().is_empty()
            || request.route_id.trim().is_empty()
            || request.route_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.action_node_id.trim().is_empty()
            || request.action_node_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.scene_equipment.trim().is_empty()
            || request.scene_equipment.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || geo_region.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || device_os.chars().count() > MAX_CONTEXT_FIELD_LENGTH
        {
            return Ok(center::CampaignList { items: Vec::new() });
        }
        let mut client = self.center.clone();
        Ok(client
            .eligible(
                bookway_runtime::grpc_service_request(center::EligibleRequest {
                    user_id: request.user_id,
                    placement: request.placement,
                    domain: request.domain,
                    limit: request.limit.clamp(1, 100),
                    route_id: request.route_id,
                    action_node_id: request.action_node_id,
                    scene_equipment: request.scene_equipment,
                    geo_region,
                    device_os,
                })
                .map_err(|error| tonic::Status::internal(error.to_string()))?,
            )
            .await?
            .into_inner())
    }
}
