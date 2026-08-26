use std::collections::{BTreeSet, HashMap};

use crate::api::pb;
use async_trait::async_trait;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct EligibleQuery<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) placement: &'a str,
    pub(crate) domain: &'a str,
    pub(crate) route_id: &'a str,
    pub(crate) action_node_id: &'a str,
    pub(crate) scene_equipment: &'a str,
    pub(crate) limit: usize,
}

// A delivery receipt is valuable only if it can be traced back to the exact
// action-node placement that selected the campaign. Keep that context in the
// dao boundary instead of trusting an upstream caller to retain it.
pub(crate) struct DecisionRegistration<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) request_id: &'a str,
    pub(crate) placement: &'a str,
    pub(crate) route_id: &'a str,
    pub(crate) action_node_id: &'a str,
    pub(crate) scene_equipment: &'a str,
    pub(crate) campaign_ids: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum DaoError {
    NotFound(String),
    Failed(String),
}

#[async_trait]
pub(crate) trait CampaignDao: Send + Sync {
    async fn create(&self, request: pb::CreateCampaignRequest) -> Result<pb::AdCampaign, DaoError>;
    async fn update(
        &self,
        campaign_id: &str,
        request: pb::UpdateCampaignRequest,
    ) -> Result<pb::AdCampaign, DaoError>;
    async fn get_for_advertiser(
        &self,
        campaign_id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, DaoError>;
    async fn list_for_advertiser(
        &self,
        advertiser_id: &str,
        limit: usize,
    ) -> Result<Vec<pb::AdCampaign>, DaoError>;
    async fn eligible(&self, query: EligibleQuery<'_>) -> Result<Vec<pb::AdCampaign>, DaoError>;
    async fn register_decisions(
        &self,
        registration: DecisionRegistration<'_>,
    ) -> Result<(), DaoError>;
    async fn record_event(
        &self,
        user_id: &str,
        request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, DaoError>;
}

#[derive(Clone)]
struct MemoryEvent {
    user_id: String,
    request_id: String,
    campaign_id: String,
    event_type: i32,
    occurred_at: OffsetDateTime,
}

#[derive(Clone)]
struct MemoryDecision {
    user_id: String,
    placement: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
    expires_at: OffsetDateTime,
}

fn campaign_select(where_clause: &str) -> String {
    format!(
        "SELECT c.id,c.advertiser_id,c.name,c.placement,c.route_id,c.action_node_id,c.scene_equipment,c.title,c.body,c.image_url,c.landing_url,c.target_domains,c.status,c.pricing_model,c.bid_micros,c.daily_budget_micros,COALESCE(stats.spent_micros,0) AS spent_today_micros,c.frequency_cap,c.predicted_ctr,c.predicted_cvr,c.global_frequency_cap,c.impressions,c.clicks,c.starts_at,c.ends_at,c.created_at,c.updated_at FROM ad_campaigns c LEFT JOIN ad_campaign_daily_stats stats ON stats.campaign_id=c.id AND stats.stat_date=current_date {where_clause}"
    )
}

fn unique_campaign_ids(campaign_ids: &[String]) -> Result<Vec<String>, DaoError> {
    let values = campaign_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if values.len() != campaign_ids.len() {
        return Err(DaoError::Failed(
            "campaign ids must be non-empty and unique".to_string(),
        ));
    }
    Ok(values.into_iter().collect())
}

fn campaign_matches_registration(
    campaign: &pb::AdCampaign,
    registration: &DecisionRegistration<'_>,
    now: OffsetDateTime,
) -> bool {
    campaign.status == pb::CampaignStatus::Active as i32
        && campaign.placement == registration.placement
        && campaign.route_id == registration.route_id
        && campaign.action_node_id == registration.action_node_id
        && campaign.scene_equipment == registration.scene_equipment
        && campaign_starts(campaign).is_none_or(|start| start <= now)
        && campaign_ends(campaign).is_none_or(|end| end > now)
}

fn decision_matches_registration(
    decision: &MemoryDecision,
    registration: &DecisionRegistration<'_>,
) -> bool {
    decision.user_id == registration.user_id
        && decision.placement == registration.placement
        && decision.route_id == registration.route_id
        && decision.action_node_id == registration.action_node_id
        && decision.scene_equipment == registration.scene_equipment
}

fn new_campaign(request: pb::CreateCampaignRequest) -> pb::AdCampaign {
    let now = OffsetDateTime::now_utc();
    pb::AdCampaign {
        id: Uuid::now_v7().to_string(),
        advertiser_id: request.advertiser_id,
        name: request.name,
        placement: request.placement,
        route_id: request.route_id,
        action_node_id: request.action_node_id,
        scene_equipment: request.scene_equipment,
        title: request.title,
        body: request.body,
        image_url: request.image_url,
        landing_url: request.landing_url,
        target_domains: request.target_domains,
        status: pb::CampaignStatus::Draft as i32,
        pricing_model: request.pricing_model,
        bid_micros: request.bid_micros,
        daily_budget_micros: request.daily_budget_micros,
        spent_today_micros: 0,
        frequency_cap: request.frequency_cap,
        predicted_ctr: request.predicted_ctr,
        predicted_cvr: request.predicted_cvr,
        global_frequency_cap: request.global_frequency_cap,
        impressions: 0,
        clicks: 0,
        starts_at: request.starts_at,
        ends_at: request.ends_at,
        created_at: timestamp(now),
        updated_at: timestamp(now),
    }
}

fn apply_update(campaign: &mut pb::AdCampaign, request: pb::UpdateCampaignRequest) {
    if let Some(value) = request.status {
        campaign.status = value;
    }
    if let Some(value) = request.name {
        campaign.name = value;
    }
    if let Some(value) = request.title {
        campaign.title = value;
    }
    if let Some(value) = request.body {
        campaign.body = value;
    }
    if let Some(value) = request.image_url {
        campaign.image_url = value;
    }
    if let Some(value) = request.landing_url {
        campaign.landing_url = value;
    }
    if let Some(value) = request.target_domains {
        campaign.target_domains = value.values;
    }
    if let Some(value) = request.bid_micros {
        campaign.bid_micros = value;
    }
    if let Some(value) = request.daily_budget_micros {
        campaign.daily_budget_micros = value;
    }
    if let Some(value) = request.frequency_cap {
        campaign.frequency_cap = value;
    }
    if let Some(value) = request.predicted_ctr {
        campaign.predicted_ctr = value;
    }
    if let Some(value) = request.predicted_cvr {
        campaign.predicted_cvr = value;
    }
    if let Some(value) = request.global_frequency_cap {
        campaign.global_frequency_cap = value;
    }
    if let Some(value) = request.scene_equipment {
        campaign.scene_equipment = value;
    }
    if let Some(value) = request.starts_at {
        campaign.starts_at = Some(value);
    }
    if let Some(value) = request.ends_at {
        campaign.ends_at = Some(value);
    }
    campaign.updated_at = timestamp(OffsetDateTime::now_utc());
}

fn event_cost(campaign: &pb::AdCampaign, event_type: i32) -> i64 {
    match (campaign.pricing_model, event_type) {
        (pricing, event)
            if pricing == pb::PricingModel::Cpm as i32
                && event == pb::EventType::Impression as i32 =>
        {
            (campaign.bid_micros.saturating_add(999)) / 1_000
        }
        (pricing, event)
            if pricing == pb::PricingModel::Cpc as i32 && event == pb::EventType::Click as i32 =>
        {
            campaign.bid_micros
        }
        _ => 0,
    }
}

fn campaign_starts(campaign: &pb::AdCampaign) -> Option<OffsetDateTime> {
    campaign
        .starts_at
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
}
fn campaign_ends(campaign: &pb::AdCampaign) -> Option<OffsetDateTime> {
    campaign
        .ends_at
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
}
fn date_key(value: OffsetDateTime) -> String {
    value.date().to_string()
}
fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
fn parse_timestamp(value: Option<&str>) -> Result<Option<OffsetDateTime>, DaoError> {
    value
        .map(|value| {
            OffsetDateTime::parse(value, &Rfc3339)
                .map_err(|error| DaoError::Failed(error.to_string()))
        })
        .transpose()
}
fn database(error: sqlx::Error) -> DaoError {
    DaoError::Failed(error.to_string())
}
fn status_name(value: i32) -> &'static str {
    match pb::CampaignStatus::try_from(value).unwrap_or(pb::CampaignStatus::Draft) {
        pb::CampaignStatus::Draft => "draft",
        pb::CampaignStatus::Active => "active",
        pb::CampaignStatus::Paused => "paused",
        pb::CampaignStatus::Ended => "ended",
    }
}
fn pricing_name(value: i32) -> &'static str {
    match pb::PricingModel::try_from(value).unwrap_or(pb::PricingModel::Cpm) {
        pb::PricingModel::Cpm => "cpm",
        pb::PricingModel::Cpc => "cpc",
    }
}
fn event_name(value: i32) -> &'static str {
    match pb::EventType::try_from(value).unwrap_or(pb::EventType::Impression) {
        pb::EventType::Impression => "impression",
        pb::EventType::Click => "click",
    }
}
fn parse_status(value: &str) -> Result<pb::CampaignStatus, DaoError> {
    match value {
        "draft" => Ok(pb::CampaignStatus::Draft),
        "active" => Ok(pb::CampaignStatus::Active),
        "paused" => Ok(pb::CampaignStatus::Paused),
        "ended" => Ok(pb::CampaignStatus::Ended),
        _ => Err(DaoError::Failed(format!("unknown campaign status {value}"))),
    }
}
fn parse_pricing(value: &str) -> Result<pb::PricingModel, DaoError> {
    match value {
        "cpm" => Ok(pb::PricingModel::Cpm),
        "cpc" => Ok(pb::PricingModel::Cpc),
        _ => Err(DaoError::Failed(format!("unknown pricing model {value}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::{CampaignDao, DaoError, DecisionRegistration, MemoryCampaignDao};
    use crate::api::pb;

    fn campaign_request() -> pb::CreateCampaignRequest {
        pb::CreateCampaignRequest {
            advertiser_id: "advertiser-1".to_string(),
            name: "测试活动".to_string(),
            placement: "feed".to_string(),
            route_id: "route-1".to_string(),
            action_node_id: "node-1".to_string(),
            scene_equipment: "轻量背包".to_string(),
            title: "广告标题".to_string(),
            body: String::new(),
            image_url: String::new(),
            landing_url: "https://example.com".to_string(),
            target_domains: Vec::new(),
            pricing_model: pb::PricingModel::Cpm as i32,
            bid_micros: 1_000,
            daily_budget_micros: 10_000,
            frequency_cap: 1,
            starts_at: None,
            ends_at: None,
            predicted_ctr: 0.1,
            predicted_cvr: 0.2,
            global_frequency_cap: 0,
        }
    }

    fn registration<'a>(
        user_id: &'a str,
        request_id: &'a str,
        campaign_ids: Vec<String>,
    ) -> DecisionRegistration<'a> {
        DecisionRegistration {
            user_id,
            request_id,
            placement: "feed",
            route_id: "route-1",
            action_node_id: "node-1",
            scene_equipment: "轻量背包",
            campaign_ids,
        }
    }

    #[tokio::test]
    async fn accepts_only_tracked_and_once_per_decision_event() {
        let dao = MemoryCampaignDao::default();
        let campaign = dao
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");

        let untracked = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "event-untracked".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("untracked receipt has a valid response");
        assert!(!untracked.accepted);

        dao.register_decisions(registration(
            "user-1",
            "request-1",
            vec![campaign.id.clone()],
        ))
        .await
        .expect("decision should be registered");
        let accepted = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "event-accepted".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("tracked receipt should be recorded");
        assert!(accepted.accepted);
        assert!(!accepted.duplicate);

        let duplicate = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "event-retry".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("duplicate receipt should be recognized");
        assert!(duplicate.accepted);
        assert!(duplicate.duplicate);

        let second_campaign = dao
            .create(campaign_request())
            .await
            .expect("second campaign should be created");
        dao.update(
            &second_campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: second_campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("second campaign should be activated");
        let changed_campaign_set = dao
            .register_decisions(registration(
                "user-1",
                "request-1",
                vec![campaign.id.clone(), second_campaign.id],
            ))
            .await;
        assert!(matches!(
            changed_campaign_set,
            Err(DaoError::Failed(message))
                if message == "request id already belongs to a different decision context or campaign set"
        ));

        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Paused as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be pausable");
        let retry_after_pause = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "event-accepted".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("idempotent retry should remain readable after a pause");
        assert!(retry_after_pause.accepted);
        assert!(retry_after_pause.duplicate);
    }

    #[tokio::test]
    async fn an_expired_request_id_cannot_be_rebound_to_a_new_decision() {
        let dao = MemoryCampaignDao::default();
        let campaign = dao
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");
        let request_id = "expired-request";
        dao.register_decisions(registration(
            "user-1",
            request_id,
            vec![campaign.id.clone()],
        ))
        .await
        .expect("decision should be registered");
        dao.expire_decision_for_test(request_id, &campaign.id).await;
        let result = dao
            .register_decisions(registration("user-1", request_id, vec![campaign.id]))
            .await;
        assert!(matches!(result, Err(DaoError::Failed(message)) if message.contains("expired")));
    }

    #[tokio::test]
    async fn requires_an_accepted_impression_before_accepting_a_click() {
        let dao = MemoryCampaignDao::default();
        let mut request = campaign_request();
        request.pricing_model = pb::PricingModel::Cpc as i32;
        let campaign = dao
            .create(request)
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");
        dao.register_decisions(registration(
            "user-1",
            "request-1",
            vec![campaign.id.clone()],
        ))
        .await
        .expect("decision should be registered");

        let click_before_impression = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "click-before-impression".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Click as i32,
                },
            )
            .await
            .expect("early click has a valid response");
        assert!(!click_before_impression.accepted);
        assert!(!click_before_impression.duplicate);
        let after_rejected_click = dao
            .get_for_advertiser(&campaign.id, &campaign.advertiser_id)
            .await
            .expect("campaign should remain readable");
        assert_eq!(after_rejected_click.spent_today_micros, 0);
        assert_eq!(after_rejected_click.clicks, 0);

        let impression = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "impression-1".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("impression should be recorded");
        assert!(impression.accepted);

        let click_after_impression = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "click-after-impression".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Click as i32,
                },
            )
            .await
            .expect("click after impression should be recorded");
        assert!(click_after_impression.accepted);
        assert!(!click_after_impression.duplicate);
        let after_click = dao
            .get_for_advertiser(&campaign.id, &campaign.advertiser_id)
            .await
            .expect("campaign should remain readable");
        assert_eq!(after_click.spent_today_micros, campaign.bid_micros);
        assert_eq!(after_click.clicks, 1);
    }

    #[tokio::test]
    async fn global_frequency_cap_blocks_a_second_user() {
        let dao = MemoryCampaignDao::default();
        let mut request = campaign_request();
        request.global_frequency_cap = 1;
        let campaign = dao
            .create(request)
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");
        dao.register_decisions(registration(
            "user-1",
            "request-1",
            vec![campaign.id.clone()],
        ))
        .await
        .expect("first decision should be tracked");
        assert!(
            dao.record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "impression-1".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id.clone(),
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("first impression should be accepted")
            .accepted
        );
        dao.register_decisions(registration(
            "user-2",
            "request-2",
            vec![campaign.id.clone()],
        ))
        .await
        .expect("second decision should be tracked");
        assert!(
            !dao.record_event(
                "user-2",
                pb::RecordEventRequest {
                    user_id: "user-2".to_string(),
                    event_id: "impression-2".to_string(),
                    request_id: "request-2".to_string(),
                    campaign_id: campaign.id,
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("second impression should be rejected")
            .accepted
        );
    }

    #[tokio::test]
    async fn event_id_cannot_be_reused_across_delivery_contexts() {
        let dao = MemoryCampaignDao::default();
        let campaign = dao
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");
        dao.register_decisions(registration(
            "user-1",
            "request-1",
            vec![campaign.id.clone()],
        ))
        .await
        .expect("first decision should be tracked");
        dao.record_event(
            "user-1",
            pb::RecordEventRequest {
                user_id: "user-1".to_string(),
                event_id: "shared-event".to_string(),
                request_id: "request-1".to_string(),
                campaign_id: campaign.id.clone(),
                event_type: pb::EventType::Impression as i32,
            },
        )
        .await
        .expect("first event should be accepted");
        dao.register_decisions(registration(
            "user-2",
            "request-2",
            vec![campaign.id.clone()],
        ))
        .await
        .expect("second decision should be tracked");
        let conflict = dao
            .record_event(
                "user-2",
                pb::RecordEventRequest {
                    user_id: "user-2".to_string(),
                    event_id: "shared-event".to_string(),
                    request_id: "request-2".to_string(),
                    campaign_id: campaign.id,
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await;
        assert!(matches!(
            conflict,
            Err(DaoError::Failed(message))
                if message == "event id was already used for a different delivery event"
        ));
    }

    #[tokio::test]
    async fn rejects_a_decision_that_does_not_match_the_campaign_action_node() {
        let dao = MemoryCampaignDao::default();
        let campaign = dao
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");

        let rejected = dao
            .register_decisions(DecisionRegistration {
                action_node_id: "other-node",
                ..registration("user-1", "request-wrong-context", vec![campaign.id.clone()])
            })
            .await;
        assert!(matches!(rejected, Err(DaoError::Failed(_))));

        let receipt = dao
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "event-wrong-context".to_string(),
                    request_id: "request-wrong-context".to_string(),
                    campaign_id: campaign.id,
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("invalid decision should return a receipt");
        assert!(!receipt.accepted);
    }

    #[tokio::test]
    async fn rejects_a_decision_that_does_not_match_scene_equipment() {
        let dao = MemoryCampaignDao::default();
        let campaign = dao
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        dao.update(
            &campaign.id,
            pb::UpdateCampaignRequest {
                advertiser_id: campaign.advertiser_id.clone(),
                status: Some(pb::CampaignStatus::Active as i32),
                ..Default::default()
            },
        )
        .await
        .expect("campaign should be activated");

        let rejected = dao
            .register_decisions(DecisionRegistration {
                scene_equipment: "wrong equipment",
                ..registration("user-1", "request-wrong-equipment", vec![campaign.id])
            })
            .await;
        assert!(matches!(rejected, Err(DaoError::Failed(_))));
    }

    #[tokio::test]
    async fn advertiser_catalog_is_isolated_for_reads_and_updates() {
        let dao = MemoryCampaignDao::default();
        let first = dao
            .create(campaign_request())
            .await
            .expect("first advertiser campaign should be created");
        let mut second_request = campaign_request();
        second_request.advertiser_id = "advertiser-2".to_string();
        let second = dao
            .create(second_request)
            .await
            .expect("second advertiser campaign should be created");

        let first_campaigns = dao
            .list_for_advertiser("advertiser-1", 20)
            .await
            .expect("first advertiser list should load");
        assert_eq!(first_campaigns.len(), 1);
        assert_eq!(first_campaigns[0].id, first.id);
        assert!(matches!(
            dao.get_for_advertiser(&first.id, "advertiser-2").await,
            Err(DaoError::NotFound(_))
        ));
        assert!(matches!(
            dao.update(
                &second.id,
                pb::UpdateCampaignRequest {
                    advertiser_id: "advertiser-1".to_string(),
                    name: Some("attempted cross-account update".to_string()),
                    ..Default::default()
                },
            )
            .await,
            Err(DaoError::NotFound(_))
        ));
    }
}

#[path = "memory_campaign_dao.rs"]
mod memory_campaign_dao;
pub(crate) use memory_campaign_dao::MemoryCampaignDao;
#[path = "postgres_campaign_dao.rs"]
mod postgres_campaign_dao;
pub(crate) use postgres_campaign_dao::PostgresCampaignDao;
