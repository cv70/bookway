use bookway_ad_center_api::pb::{self as center, ad_center_client::AdCenterClient};
use bookway_ad_rank_api::pb::{self as rank, ad_rank_client::AdRankClient};
use bookway_ad_recall_api::pb::{self as recall, ad_recall_client::AdRecallClient};

use crate::Config;
use thiserror::Error;
use uuid::Uuid;
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
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let recall = AdRecallClient::connect(config.recall_url.clone()).await?;
        let rank = AdRankClient::connect(config.rank_url.clone()).await?;
        let center = AdCenterClient::connect(config.center_url.clone()).await?;
        Ok(Self {
            config,
            recall,
            rank,
            center,
        })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn decide(
        &self,
        request: crate::api::pb::DecisionRequest,
    ) -> Result<crate::api::pb::DecisionResponse, AdMainError> {
        if request.user_id.trim().is_empty() || request.placement.trim().is_empty() {
            return Err(AdMainError::Validation(
                "user_id and placement are required".to_string(),
            ));
        }
        let limit = request.limit.unwrap_or(1).clamp(
            1,
            u32::try_from(self.config.max_decisions.clamp(1, 10)).unwrap_or(10),
        ) as usize;
        let mut recall_client = self.recall.clone();
        let candidates = recall_client
            .recall(service_request(
                "ad-recall",
                recall::RecallRequest {
                    user_id: request.user_id.clone(),
                    placement: request.placement.clone(),
                    domain: request.domain.clone().unwrap_or_default(),
                    limit: u32::try_from(limit.saturating_mul(4)).unwrap_or(u32::MAX),
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
                },
            )?)
            .await
            .map_err(|error| upstream_error("ad-rank", error))?
            .into_inner();
        let request_id = Uuid::now_v7().to_string();
        let items = ranked
            .items
            .into_iter()
            .take(limit)
            .filter_map(|item| {
                item.campaign.map(|campaign| crate::api::pb::AdDecision {
                    request_id: request_id.clone(),
                    campaign_id: campaign.id,
                    placement: campaign.placement,
                    title: campaign.title,
                    body: campaign.body,
                    image_url: campaign.image_url,
                    landing_url: campaign.landing_url,
                    score: item.score,
                    model_version: ranked.model_version.clone(),
                })
            })
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
                    },
                )?)
                .await
                .map_err(|error| upstream_error("ad-center", error))?;
        }
        Ok(crate::api::pb::DecisionResponse {
            request_id,
            items,
            degraded: ranked.degraded,
        })
    }
    pub(crate) async fn report_event(
        &self,
        request: center::RecordEventRequest,
    ) -> Result<center::EventReceipt, AdMainError> {
        if request.user_id.trim().is_empty()
            || request.event_id.trim().is_empty()
            || request.request_id.trim().is_empty()
            || request.campaign_id.trim().is_empty()
        {
            return Err(AdMainError::Validation(
                "user_id, event_id, request_id and campaign_id are required".to_string(),
            ));
        }
        let mut client = self.center.clone();
        client
            .record_event(service_request("ad-center", request)?)
            .await
            .map(|response| response.into_inner())
            .map_err(|error| upstream_error("ad-center", error))
    }
}
fn service_request<T>(service: &'static str, value: T) -> Result<tonic::Request<T>, AdMainError> {
    bookway_runtime::grpc_service_request(value)
        .map_err(|error| AdMainError::Upstream(format!("{service} request failed: {error}")))
}
fn upstream_error(service: &'static str, error: tonic::Status) -> AdMainError {
    AdMainError::Upstream(format!("{service} request failed: {error}"))
}
