use std::sync::Arc;

use crate::api::pb;
use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
use thiserror::Error;

use crate::{
    Config,
    datasource::{
        CampaignRepository, EligibleQuery, MemoryCampaignRepository, PostgresCampaignRepository,
        RepositoryError,
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
    repository: Arc<dyn CampaignRepository>,
    bbs_link: BbsLinkClient<tonic::transport::Channel>,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: Arc<dyn CampaignRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCampaignRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCampaignRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let bbs_link = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            config,
            repository,
            bbs_link,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) async fn create_campaign(
        &self,
        request: pb::CreateCampaignRequest,
    ) -> Result<pb::AdCampaign, AdCenterError> {
        validate_campaign(&request)?;
        self.validate_public_action_node(&request.route_id, &request.action_node_id)
            .await?;
        self.repository
            .create(request)
            .await
            .map_err(repository_error)
    }

    pub(crate) async fn update_campaign(
        &self,
        request: pb::UpdateCampaignRequest,
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
        if request
            .scene_equipment
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
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
        if let Some(status) = request.status {
            pb::CampaignStatus::try_from(status)
                .map_err(|_| AdCenterError::Validation("invalid campaign status".to_string()))?;
        }
        let campaign_id = request.campaign_id.clone();
        self.repository
            .update(&campaign_id, request)
            .await
            .map_err(repository_error)
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
        self.repository
            .get_for_advertiser(&request.campaign_id, &request.advertiser_id)
            .await
            .map_err(repository_error)
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
        self.repository
            .list_for_advertiser(
                &request.advertiser_id,
                usize::try_from(request.limit.unwrap_or(50).clamp(1, 100)).unwrap_or(100),
            )
            .await
            .map(|items| pb::CampaignList { items })
            .map_err(repository_error)
    }

    pub(crate) async fn eligible(
        &self,
        request: pb::EligibleRequest,
    ) -> Result<pb::CampaignList, AdCenterError> {
        if request.user_id.trim().is_empty() || request.placement.trim().is_empty() {
            return Err(AdCenterError::Validation(
                "user_id and placement are required".to_string(),
            ));
        }
        self.repository
            .eligible(EligibleQuery {
                user_id: &request.user_id,
                placement: &request.placement,
                domain: &request.domain,
                route_id: &request.route_id,
                action_node_id: &request.action_node_id,
                limit: usize::try_from(request.limit.clamp(1, 100)).unwrap_or(100),
            })
            .await
            .map(|items| pb::CampaignList { items })
            .map_err(repository_error)
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
        self.repository
            .record_event(&user_id, request)
            .await
            .map_err(repository_error)
    }

    pub(crate) async fn register_decisions(
        &self,
        request: pb::RegisterDecisionRequest,
    ) -> Result<(), AdCenterError> {
        if request.user_id.trim().is_empty()
            || request.request_id.trim().is_empty()
            || request.placement.trim().is_empty()
            || request.campaign_ids.is_empty()
            || request.campaign_ids.len() > 10
        {
            return Err(AdCenterError::Validation(
                "user_id, request_id, placement and 1-10 campaign ids are required".to_string(),
            ));
        }
        self.repository
            .register_decisions(
                &request.user_id,
                &request.request_id,
                &request.placement,
                request.campaign_ids,
            )
            .await
            .map_err(repository_error)
    }

    async fn validate_public_action_node(
        &self,
        route_id: &str,
        action_node_id: &str,
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
        if !route.route_template.as_ref().is_some_and(|template| {
            template
                .actions
                .iter()
                .any(|action| action.id == action_node_id)
        }) {
            return Err(AdCenterError::Validation(
                "广告行动节点不属于该公开路线".to_string(),
            ));
        }
        Ok(())
    }
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
    if !request.landing_url.starts_with("https://") && !request.landing_url.starts_with("http://") {
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

fn repository_error(error: RepositoryError) -> AdCenterError {
    match error {
        RepositoryError::NotFound(id) => AdCenterError::NotFound(id),
        RepositoryError::Failed(message) => AdCenterError::Repository(message),
    }
}
