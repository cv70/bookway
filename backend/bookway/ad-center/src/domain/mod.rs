use std::sync::Arc;

use crate::api::pb;
use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
use thiserror::Error;
use url::Url;

use crate::{
    Config,
    datasource::{
        CampaignDao, DaoError, DecisionRegistration, EligibleQuery, MemoryCampaignDao,
        PostgresCampaignDao,
    },
};

#[derive(Debug, Error)]
pub(crate) enum AdCenterError {
    #[error("{0}")]
    Validation(String),
    #[error("campaign {0} was not found")]
    NotFound(String),
    #[error("commercial data operation failed: {0}")]
    Repository(String),
}

#[derive(Clone)]
pub struct Domain {
    config: Config,
    dao: Arc<dyn CampaignDao>,
    bbs_link: BbsLinkClient<tonic::transport::Channel>,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let dao: Arc<dyn CampaignDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCampaignDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCampaignDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let bbs_link = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            config,
            dao,
            bbs_link,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) async fn create_campaign(
        &self,
        mut request: pb::CreateCampaignRequest,
    ) -> Result<pb::AdCampaign, AdCenterError> {
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        validate_campaign(&request)?;
        self.validate_public_action_node(
            &request.route_id,
            &request.action_node_id,
            &request.scene_equipment,
        )
        .await?;
        self.dao.create(request).await.map_err(dao_error)
    }

    pub(crate) async fn update_campaign(
        &self,
        mut request: pb::UpdateCampaignRequest,
    ) -> Result<pb::AdCampaign, AdCenterError> {
        if request.campaign_id.trim().is_empty() || request.advertiser_id.trim().is_empty() {
            return Err(AdCenterError::Validation(
                "campaign_id and advertiser_id are required".to_string(),
            ));
        }
        if request.bid_micros.is_some_and(|value| value < 0)
            || request.daily_budget_micros.is_some_and(|value| value < 0)
        {
            return Err(AdCenterError::Validation(
                "budget and bid cannot be negative".to_string(),
            ));
        }
        if request.scene_equipment.is_some() {
            request.scene_equipment = request
                .scene_equipment
                .take()
                .map(|value| scene_equipment_key(&value));
        }
        if request
            .scene_equipment
            .as_ref()
            .is_some_and(String::is_empty)
        {
            return Err(AdCenterError::Validation(
                "scene equipment cannot be empty".to_string(),
            ));
        }
        if request
            .predicted_ctr
            .is_some_and(|value| !valid_probability(value))
            || request
                .predicted_cvr
                .is_some_and(|value| !valid_probability(value))
        {
            return Err(AdCenterError::Validation(
                "predicted CTR/CVR must be finite values between 0 and 1".to_string(),
            ));
        }
        if request
            .landing_url
            .as_deref()
            .is_some_and(|value| !valid_landing_url(value))
        {
            return Err(AdCenterError::Validation(
                "landing_url must be an http(s) URL".to_string(),
            ));
        }
        let requested_status = request
            .status
            .map(pb::CampaignStatus::try_from)
            .transpose()
            .map_err(|_| AdCenterError::Validation("invalid campaign status".to_string()))?;
        let campaign_id = request.campaign_id.clone();
        let campaign = self
            .dao
            .get_for_advertiser(&campaign_id, &request.advertiser_id)
            .await
            .map_err(dao_error)?;
        if needs_current_action_context(
            request.scene_equipment.is_some(),
            requested_status,
            campaign.status,
        ) {
            self.validate_public_action_node(
                &campaign.route_id,
                &campaign.action_node_id,
                request
                    .scene_equipment
                    .as_deref()
                    .unwrap_or(&campaign.scene_equipment),
            )
            .await?;
        }
        self.dao
            .update(&campaign_id, request)
            .await
            .map_err(dao_error)
    }

    pub(crate) async fn campaign(
        &self,
        request: pb::CampaignIdRequest,
    ) -> Result<pb::AdCampaign, AdCenterError> {
        if request.campaign_id.trim().is_empty() || request.advertiser_id.trim().is_empty() {
            return Err(AdCenterError::Validation(
                "campaign_id and advertiser_id are required".to_string(),
            ));
        }
        self.dao
            .get_for_advertiser(&request.campaign_id, &request.advertiser_id)
            .await
            .map_err(dao_error)
    }

    pub(crate) async fn campaigns(
        &self,
        request: pb::AdvertiserCampaignQuery,
    ) -> Result<pb::CampaignList, AdCenterError> {
        if request.advertiser_id.trim().is_empty() {
            return Err(AdCenterError::Validation(
                "advertiser_id is required".to_string(),
            ));
        }
        self.dao
            .list_for_advertiser(
                &request.advertiser_id,
                usize::try_from(request.limit.unwrap_or(50).clamp(1, 100)).unwrap_or(100),
            )
            .await
            .map(|items| pb::CampaignList { items })
            .map_err(dao_error)
    }

    pub(crate) async fn eligible(
        &self,
        mut request: pb::EligibleRequest,
    ) -> Result<pb::CampaignList, AdCenterError> {
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if request.user_id.trim().is_empty()
            || request.placement.trim().is_empty()
            || request.route_id.trim().is_empty()
            || request.action_node_id.trim().is_empty()
            || request.scene_equipment.trim().is_empty()
        {
            return Err(AdCenterError::Validation(
                "user_id, placement, route, action node and scene equipment are required"
                    .to_string(),
            ));
        }
        // Campaign rows are durable, but a route can be restricted or its
        // action vocabulary can change after activation. Re-read the public
        // route on every eligibility request so stale commercial state can
        // never become a served advertisement.
        self.validate_public_action_node(
            &request.route_id,
            &request.action_node_id,
            &request.scene_equipment,
        )
        .await?;
        self.dao
            .eligible(EligibleQuery {
                user_id: &request.user_id,
                placement: &request.placement,
                domain: &request.domain,
                route_id: &request.route_id,
                action_node_id: &request.action_node_id,
                scene_equipment: &request.scene_equipment,
                limit: usize::try_from(request.limit.clamp(1, 100)).unwrap_or(100),
            })
            .await
            .map(|items| pb::CampaignList { items })
            .map_err(dao_error)
    }

    pub(crate) async fn record_event(
        &self,
        request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, AdCenterError> {
        if request.user_id.trim().is_empty()
            || request.event_id.trim().is_empty()
            || request.request_id.trim().is_empty()
            || request.campaign_id.trim().is_empty()
        {
            return Err(AdCenterError::Validation(
                "user_id, event_id, request_id and campaign_id are required".to_string(),
            ));
        }
        pb::EventType::try_from(request.event_type)
            .map_err(|_| AdCenterError::Validation("invalid event type".to_string()))?;
        let user_id = request.user_id.clone();
        self.dao
            .record_event(&user_id, request)
            .await
            .map_err(dao_error)
    }

    pub(crate) async fn register_decisions(
        &self,
        mut request: pb::RegisterDecisionRequest,
    ) -> Result<(), AdCenterError> {
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if request.user_id.trim().is_empty()
            || request.request_id.trim().is_empty()
            || request.placement.trim().is_empty()
            || request.route_id.trim().is_empty()
            || request.action_node_id.trim().is_empty()
            || request.scene_equipment.trim().is_empty()
            || request.campaign_ids.is_empty()
            || request.campaign_ids.len() > 10
        {
            return Err(AdCenterError::Validation(
                "user_id, request_id, placement, route, action node and 1-10 campaign ids are required".to_string(),
            ));
        }
        self.validate_public_action_node(
            &request.route_id,
            &request.action_node_id,
            &request.scene_equipment,
        )
        .await?;
        self.dao
            .register_decisions(DecisionRegistration {
                user_id: &request.user_id,
                request_id: &request.request_id,
                placement: &request.placement,
                route_id: &request.route_id,
                action_node_id: &request.action_node_id,
                scene_equipment: &request.scene_equipment,
                campaign_ids: request.campaign_ids,
            })
            .await
            .map_err(dao_error)
    }

    async fn validate_public_action_node(
        &self,
        route_id: &str,
        action_node_id: &str,
        scene_equipment: &str,
    ) -> Result<(), AdCenterError> {
        let mut client = self.bbs_link.clone();
        let route = client
            .get_public(
                bookway_runtime::grpc_service_request(bbs_link::IdRequest {
                    id: route_id.to_string(),
                })
                .map_err(|error| AdCenterError::Repository(error.to_string()))?,
            )
            .await
            .map_err(|error| match error.code() {
                tonic::Code::NotFound => AdCenterError::NotFound(route_id.to_string()),
                _ => AdCenterError::Repository(format!("bbs-link get_public failed: {error}")),
            })?
            .into_inner();
        if route.content_type != bbs_link::ContentType::Route as i32 {
            return Err(AdCenterError::Validation(
                "广告只能挂载到公开路线行动节点".to_string(),
            ));
        }
        let action = route.route_template.as_ref().and_then(|template| {
            template
                .actions
                .iter()
                .find(|action| action.id == action_node_id)
        });
        let Some(action) = action else {
            return Err(AdCenterError::Validation(
                "广告行动节点不属于该公开路线".to_string(),
            ));
        };
        if !action
            .scene_equipment
            .iter()
            .any(|value| scene_equipment_key(value) == scene_equipment_key(scene_equipment))
        {
            return Err(AdCenterError::Validation(
                "广告场景装备未在行动节点中声明".to_string(),
            ));
        }
        Ok(())
    }
}

fn scene_equipment_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn needs_current_action_context(
    updates_scene_equipment: bool,
    requested_status: Option<pb::CampaignStatus>,
    current_status: i32,
) -> bool {
    updates_scene_equipment
        || requested_status == Some(pb::CampaignStatus::Active)
        || current_status == pb::CampaignStatus::Active as i32
}

fn validate_campaign(request: &pb::CreateCampaignRequest) -> Result<(), AdCenterError> {
    if request.advertiser_id.trim().is_empty()
        || request.name.trim().is_empty()
        || request.placement.trim().is_empty()
        || request.route_id.trim().is_empty()
        || request.action_node_id.trim().is_empty()
        || request.scene_equipment.trim().is_empty()
        || request.title.trim().is_empty()
        || request.landing_url.trim().is_empty()
    {
        return Err(AdCenterError::Validation(
            "advertiser_id, name, placement, route, action node, scene equipment, title and landing_url are required".to_string(),
        ));
    }
    if !valid_landing_url(&request.landing_url) {
        return Err(AdCenterError::Validation(
            "landing_url must be an http(s) URL".to_string(),
        ));
    }
    if request.bid_micros < 0 || request.daily_budget_micros < 0 {
        return Err(AdCenterError::Validation(
            "budget and bid cannot be negative".to_string(),
        ));
    }
    if !valid_probability(request.predicted_ctr) || !valid_probability(request.predicted_cvr) {
        return Err(AdCenterError::Validation(
            "predicted CTR/CVR must be finite values between 0 and 1".to_string(),
        ));
    }
    pb::PricingModel::try_from(request.pricing_model)
        .map_err(|_| AdCenterError::Validation("invalid pricing model".to_string()))?;
    Ok(())
}

fn valid_probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn valid_landing_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

fn dao_error(error: DaoError) -> AdCenterError {
    match error {
        DaoError::NotFound(id) => AdCenterError::NotFound(id),
        DaoError::Failed(message) => AdCenterError::Repository(message),
    }
}

#[cfg(test)]
mod tests {
    use super::{needs_current_action_context, pb, scene_equipment_key, valid_landing_url};

    #[test]
    fn scene_equipment_uses_a_stable_case_insensitive_key() {
        assert_eq!(scene_equipment_key("  Trail Shoes "), "trail shoes");
        assert_eq!(
            scene_equipment_key("TRAIL SHOES"),
            scene_equipment_key("trail shoes")
        );
    }

    #[test]
    fn active_campaign_updates_require_a_current_action_context() {
        assert!(needs_current_action_context(
            false,
            Some(pb::CampaignStatus::Active),
            pb::CampaignStatus::Draft as i32,
        ));
        assert!(needs_current_action_context(
            true,
            None,
            pb::CampaignStatus::Draft as i32,
        ));
        assert!(needs_current_action_context(
            false,
            None,
            pb::CampaignStatus::Active as i32,
        ));
        assert!(!needs_current_action_context(
            false,
            Some(pb::CampaignStatus::Paused),
            pb::CampaignStatus::Draft as i32,
        ));
    }

    #[test]
    fn landing_urls_are_limited_to_http_schemes() {
        assert!(valid_landing_url("https://example.test/path"));
        assert!(valid_landing_url("http://example.test/path"));
        assert!(!valid_landing_url("http://"));
        assert!(!valid_landing_url("https://?missing-host"));
        assert!(!valid_landing_url("javascript:alert(1)"));
        assert!(!valid_landing_url("//example.test/path"));
    }
}
