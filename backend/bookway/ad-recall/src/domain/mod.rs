use bookway_ad_center_api::pb::{self as center, ad_center_client::AdCenterClient};

use crate::Config;

#[derive(Clone)]
pub struct Domain {
    config: Config,
    center: AdCenterClient<tonic::transport::Channel>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let center = AdCenterClient::connect(config.center_url.clone()).await?;
        Ok(Self { config, center })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn recall(
        &self,
        request: crate::api::pb::RecallRequest,
    ) -> Result<center::CampaignList, tonic::Status> {
        if request.user_id.trim().is_empty()
            || request.placement.trim().is_empty()
            || request.route_id.trim().is_empty()
            || request.action_node_id.trim().is_empty()
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
                })
                .map_err(|error| tonic::Status::internal(error.to_string()))?,
            )
            .await?
            .into_inner())
    }
}
