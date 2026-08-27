use std::sync::Arc;

use crate::api::pb;
use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
use thiserror::Error;
use url::Url;

use crate::{
    Config,
    datasource::{
        CampaignDao, DaoError, DecisionRegistration, DEFAULT_USER_DAILY_TOTAL_CAP,
        DeliveryReportQuery, EligibleQuery, FrequencyGate, MemoryCampaignDao, PostgresCampaignDao,
    },
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const MAX_IDENTIFIER_LENGTH: usize = 160;
const MAX_PLACEMENT_LENGTH: usize = 80;
const MAX_CONTEXT_FIELD_LENGTH: usize = 160;

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
    gate: Option<FrequencyGate>,
    bbs_link: BbsLinkClient<tonic::transport::Channel>,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let mode = bookway_data::storage_mode()?;
        let dao: Arc<dyn CampaignDao> = match mode {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCampaignDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCampaignDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        // The gate is a delivery accelerator, not an authority: RecordEvent
        // re-adjudicates every impression against Postgres regardless. It is
        // built for durable deployments; losing it only costs the pre-filter.
        let gate = if matches!(mode, bookway_data::StorageMode::Postgres) {
            FrequencyGate::connect().await
        } else {
            None
        };
        let bbs_link =
            BbsLinkClient::new(bookway_runtime::grpc_channel(&config.bbs_link_url).await?);
        Ok(Self {
            config,
            dao,
            gate,
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
        request.advertiser_id = request.advertiser_id.trim().to_string();
        request.name = request.name.trim().to_string();
        request.placement = request.placement.trim().to_string();
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        request.title = request.title.trim().to_string();
        request.landing_url = request.landing_url.trim().to_string();
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        request.geo_regions = normalize_targeting(request.geo_regions)?;
        request.device_os = normalize_targeting(request.device_os)?;
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
        request.campaign_id = request.campaign_id.trim().to_string();
        request.advertiser_id = request.advertiser_id.trim().to_string();
        if invalid_identifier(&request.campaign_id) || invalid_identifier(&request.advertiser_id) {
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
        request.geo_regions = request
            .geo_regions
            .take()
            .map(|values| normalize_targeting(values.values).map(|normalized| pb::StringList { values: normalized }))
            .transpose()?;
        request.device_os = request
            .device_os
            .take()
            .map(|values| normalize_targeting(values.values).map(|normalized| pb::StringList { values: normalized }))
            .transpose()?;
        if request
            .scene_equipment
            .as_deref()
            .is_some_and(|value| value.chars().count() > MAX_CONTEXT_FIELD_LENGTH)
        {
            return Err(AdCenterError::Validation(
                "campaign context fields are too long".to_string(),
            ));
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
        validate_schedule(request.starts_at.as_deref(), request.ends_at.as_deref())?;
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
        validate_schedule(
            request
                .starts_at
                .as_deref()
                .or(campaign.starts_at.as_deref()),
            request.ends_at.as_deref().or(campaign.ends_at.as_deref()),
        )?;
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
        mut request: pb::CampaignIdRequest,
    ) -> Result<pb::AdCampaign, AdCenterError> {
        request.campaign_id = request.campaign_id.trim().to_string();
        request.advertiser_id = request.advertiser_id.trim().to_string();
        if invalid_identifier(&request.campaign_id) || invalid_identifier(&request.advertiser_id) {
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
        mut request: pb::AdvertiserCampaignQuery,
    ) -> Result<pb::CampaignList, AdCenterError> {
        request.advertiser_id = request.advertiser_id.trim().to_string();
        if invalid_identifier(&request.advertiser_id) {
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
        request.user_id = request.user_id.trim().to_string();
        request.placement = request.placement.trim().to_string();
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        request.geo_region = delivery_context_key(&request.geo_region);
        request.device_os = delivery_context_key(&request.device_os);
        if invalid_identifier(&request.user_id)
            || request.placement.trim().is_empty()
            || request.placement.chars().count() > MAX_PLACEMENT_LENGTH
            || request.route_id.trim().is_empty()
            || request.route_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.action_node_id.trim().is_empty()
            || request.action_node_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.scene_equipment.trim().is_empty()
            || request.scene_equipment.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.geo_region.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.device_os.chars().count() > MAX_CONTEXT_FIELD_LENGTH
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
        let items = self.eligible_items(&request).await?;
        Ok(pb::CampaignList { items })
    }

    /// Auction input pipeline. With the gate healthy: cheap targeting
    /// candidates, then Redis count comparison. Any gate trouble — absent at
    /// startup, degraded mid-flight — reruns `dao.eligible`, whose SQL is the
    /// authoritative adjudication.
    async fn eligible_items(
        &self,
        request: &pb::EligibleRequest,
    ) -> Result<Vec<pb::AdCampaign>, AdCenterError> {
        let query = || EligibleQuery {
            user_id: &request.user_id,
            placement: &request.placement,
            domain: &request.domain,
            route_id: &request.route_id,
            action_node_id: &request.action_node_id,
            scene_equipment: &request.scene_equipment,
            geo_region: &request.geo_region,
            device_os: &request.device_os,
            limit: usize::try_from(request.limit.clamp(1, 100)).unwrap_or(100),
        };
        let Some(gate) = &self.gate else {
            return self.dao.eligible(query()).await.map_err(dao_error);
        };
        let candidates = self
            .dao
            .eligible_candidates(query())
            .await
            .map_err(dao_error)?;
        let user_daily_total_cap = match self.dao.user_daily_total_cap().await {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "guardrail cap lookup degraded; using the seeded default"
                );
                DEFAULT_USER_DAILY_TOTAL_CAP
            }
        };
        match gate.prefilter(&candidates, &request.user_id, user_daily_total_cap).await {
            Some(items) => Ok(items),
            None => self.dao.eligible(query()).await.map_err(dao_error),
        }
    }

    pub(crate) async fn record_event(
        &self,
        mut request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, AdCenterError> {
        request.user_id = request.user_id.trim().to_string();
        request.event_id = request.event_id.trim().to_string();
        request.request_id = request.request_id.trim().to_string();
        request.campaign_id = request.campaign_id.trim().to_string();
        if invalid_identifier(&request.user_id)
            || invalid_identifier(&request.event_id)
            || invalid_identifier(&request.request_id)
            || invalid_identifier(&request.campaign_id)
        {
            return Err(AdCenterError::Validation(
                "user_id, event_id, request_id and campaign_id are required".to_string(),
            ));
        }
        pb::EventType::try_from(request.event_type)
            .map_err(|_| AdCenterError::Validation("invalid event type".to_string()))?;
        let user_id = request.user_id.clone();
        let campaign_id = request.campaign_id.clone();
        let is_impression = request.event_type == pb::EventType::Impression as i32;
        let receipt = self
            .dao
            .record_event(&user_id, request)
            .await
            .map_err(dao_error)?;
        // The authoritative acceptance is already durable; the Redis bump is a
        // best-effort accelerator for future pre-filters.
        if receipt.accepted && !receipt.duplicate && is_impression && let Some(gate) = &self.gate {
            gate.record_impression(&user_id, &campaign_id).await;
        }
        Ok(receipt)
    }

    pub(crate) async fn register_decisions(
        &self,
        mut request: pb::RegisterDecisionRequest,
    ) -> Result<(), AdCenterError> {
        request.user_id = request.user_id.trim().to_string();
        request.request_id = request.request_id.trim().to_string();
        request.placement = request.placement.trim().to_string();
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if invalid_identifier(&request.user_id)
            || request.request_id.trim().is_empty()
            || request.request_id.chars().count() > MAX_IDENTIFIER_LENGTH
            || request.placement.trim().is_empty()
            || request.placement.chars().count() > MAX_PLACEMENT_LENGTH
            || request.route_id.trim().is_empty()
            || request.route_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.action_node_id.trim().is_empty()
            || request.action_node_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.scene_equipment.trim().is_empty()
            || request.scene_equipment.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || request.campaign_ids.is_empty()
            || request.campaign_ids.len() > 10
            || request.campaign_ids.iter().any(|id| invalid_identifier(id))
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

    pub(crate) async fn delivery_guardrails(
        &self,
    ) -> Result<pb::DeliveryGuardrails, AdCenterError> {
        let user_daily_total_cap = self.dao.user_daily_total_cap().await.map_err(dao_error)?;
        Ok(pb::DeliveryGuardrails {
            user_daily_total_cap,
        })
    }

    /// Only knobs with real serving-side enforcement belong here: the
    /// per-campaign impression caps live on each campaign row and delivery
    /// pacing is serving configuration. Keeping this contract narrow means
    /// every field an operator can set actually changes behavior.
    pub(crate) async fn set_user_daily_total_cap(
        &self,
        request: pb::DeliveryGuardrails,
    ) -> Result<pb::DeliveryGuardrails, AdCenterError> {
        if !(1..=MAX_USER_DAILY_TOTAL_CAP).contains(&request.user_daily_total_cap) {
            return Err(AdCenterError::Validation(format!(
                "user_daily_total_cap must be between 1 and {MAX_USER_DAILY_TOTAL_CAP}"
            )));
        }
        let user_daily_total_cap = self
            .dao
            .set_user_daily_total_cap(request.user_daily_total_cap)
            .await
            .map_err(dao_error)?;
        Ok(pb::DeliveryGuardrails {
            user_daily_total_cap,
        })
    }

    pub(crate) async fn advertiser_delivery_report(
        &self,
        mut request: pb::AdDeliveryReportRequest,
    ) -> Result<pb::AdDeliveryReport, AdCenterError> {
        request.advertiser_id = request.advertiser_id.trim().to_string();
        if invalid_identifier(&request.advertiser_id) {
            return Err(AdCenterError::Validation(
                "advertiser_id is required".to_string(),
            ));
        }
        let from = report_day(&request.from_date)?;
        let to = report_day(&request.to_date)?;
        if from > to {
            return Err(AdCenterError::Validation(
                "report start date must not be after its end date".to_string(),
            ));
        }
        if (to - from).whole_days() > i64::from(MAX_REPORT_SPAN_DAYS) {
            return Err(AdCenterError::Validation(format!(
                "report ranges accept at most {MAX_REPORT_SPAN_DAYS} days"
            )));
        }
        let rows = self
            .dao
            .delivery_report(DeliveryReportQuery {
                advertiser_id: &request.advertiser_id,
                from_date: &from.to_string(),
                to_date: &to.to_string(),
            })
            .await
            .map_err(dao_error)?;
        Ok(pb::AdDeliveryReport {
            rows: rows
                .into_iter()
                .map(|row| pb::AdDeliveryReportRow {
                    campaign_id: row.campaign_id,
                    stat_date: row.stat_date,
                    impressions: row.impressions,
                    clicks: row.clicks,
                    spent_micros: row.spent_micros,
                })
                .collect(),
        })
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

const MAX_USER_DAILY_TOTAL_CAP: u32 = 100;
const MAX_REPORT_SPAN_DAYS: u32 = 366;

/// Strict `YYYY-MM-DD`. Anything else is a client error rather than something
/// to feed into a ledger range query.
fn report_day(value: &str) -> Result<time::Date, AdCenterError> {
    OffsetDateTime::parse(
        &format!("{}T00:00:00Z", value.trim()),
        &Rfc3339,
    )
    .map(|value| value.date())
    .map_err(|_| AdCenterError::Validation("report dates must use YYYY-MM-DD".to_string()))
}

/// Upper bound per targeting dimension (geo regions / device OS). A campaign
/// declaring more than this many values is a configuration mistake, not a
/// legitimate audience.
const MAX_TARGETING_VALUES: usize = 20;

/// Canonical delivery-context form: trimmed, lower-cased region and OS slugs
/// ("cn-bj", "ios"). Eligibility compares exact values, so both campaign
/// configuration and request context must normalize identically.
fn delivery_context_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalize_targeting(values: Vec<String>) -> Result<Vec<String>, AdCenterError> {
    let normalized = values
        .into_iter()
        .map(|value| delivery_context_key(&value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if normalized.len() > MAX_TARGETING_VALUES
        || normalized
            .iter()
            .any(|value| value.chars().count() > MAX_CONTEXT_FIELD_LENGTH)
    {
        return Err(AdCenterError::Validation(format!(
            "targeting lists accept at most {MAX_TARGETING_VALUES} values of {MAX_CONTEXT_FIELD_LENGTH} characters"
        )));
    }
    Ok(normalized)
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
    if invalid_identifier(&request.advertiser_id)
        || request.name.trim().is_empty()
        || request.placement.trim().is_empty()
        || request.placement.chars().count() > MAX_PLACEMENT_LENGTH
        || request.route_id.trim().is_empty()
        || request.route_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
        || request.action_node_id.trim().is_empty()
        || request.action_node_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
        || request.scene_equipment.trim().is_empty()
        || request.scene_equipment.chars().count() > MAX_CONTEXT_FIELD_LENGTH
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
    validate_schedule(request.starts_at.as_deref(), request.ends_at.as_deref())?;
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

fn validate_schedule(starts_at: Option<&str>, ends_at: Option<&str>) -> Result<(), AdCenterError> {
    let start = starts_at
        .map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .map_err(|_| AdCenterError::Validation("starts_at must be RFC3339".to_string()))
        })
        .transpose()?;
    let end = ends_at
        .map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .map_err(|_| AdCenterError::Validation("ends_at must be RFC3339".to_string()))
        })
        .transpose()?;
    if start.zip(end).is_some_and(|(start, end)| end <= start) {
        return Err(AdCenterError::Validation(
            "ends_at must be after starts_at".to_string(),
        ));
    }
    Ok(())
}

fn valid_probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn invalid_identifier(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.chars().count() > MAX_IDENTIFIER_LENGTH
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
    use super::{
        needs_current_action_context, pb, report_day, scene_equipment_key, valid_landing_url,
        validate_schedule,
    };

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

    #[test]
    fn campaign_schedule_is_rfc3339_and_ordered() {
        assert!(
            validate_schedule(Some("2026-01-01T00:00:00Z"), Some("2026-01-02T00:00:00Z")).is_ok()
        );
        assert!(
            validate_schedule(Some("2026-01-02T00:00:00Z"), Some("2026-01-01T00:00:00Z")).is_err()
        );
        assert!(validate_schedule(Some("not-a-time"), None).is_err());
    }

    #[test]
    fn report_days_must_be_canonical_iso_dates() {
        assert_eq!(
            report_day(" 2026-08-27 ")
                .expect("padded ISO day should parse")
                .to_string(),
            "2026-08-27"
        );
        assert!(report_day("2026-8-7").is_err());
        assert!(report_day("not-a-date").is_err());
        assert!(report_day("2026-13-40").is_err());
    }
}
