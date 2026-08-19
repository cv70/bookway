use std::collections::{BTreeSet, HashMap};

use crate::api::pb;
use async_trait::async_trait;
use sqlx::{FromRow, PgPool};
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
// repository boundary instead of trusting an upstream caller to retain it.
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
pub(crate) enum RepositoryError {
    NotFound(String),
    Failed(String),
}

#[async_trait]
pub(crate) trait CampaignRepository: Send + Sync {
    async fn create(
        &self,
        request: pb::CreateCampaignRequest,
    ) -> Result<pb::AdCampaign, RepositoryError>;
    async fn update(
        &self,
        campaign_id: &str,
        request: pb::UpdateCampaignRequest,
    ) -> Result<pb::AdCampaign, RepositoryError>;
    async fn get_for_advertiser(
        &self,
        campaign_id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, RepositoryError>;
    async fn list_for_advertiser(
        &self,
        advertiser_id: &str,
        limit: usize,
    ) -> Result<Vec<pb::AdCampaign>, RepositoryError>;
    async fn eligible(
        &self,
        query: EligibleQuery<'_>,
    ) -> Result<Vec<pb::AdCampaign>, RepositoryError>;
    async fn register_decisions(
        &self,
        registration: DecisionRegistration<'_>,
    ) -> Result<(), RepositoryError>;
    async fn record_event(
        &self,
        user_id: &str,
        request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, RepositoryError>;
}

#[derive(Default)]
pub(crate) struct MemoryCampaignRepository {
    campaigns: RwLock<HashMap<String, pb::AdCampaign>>,
    events: RwLock<HashMap<String, MemoryEvent>>,
    daily_spend: RwLock<HashMap<(String, String), i64>>,
    decisions: RwLock<HashMap<(String, String), MemoryDecision>>,
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

#[async_trait]
impl CampaignRepository for MemoryCampaignRepository {
    async fn create(
        &self,
        request: pb::CreateCampaignRequest,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        let campaign = new_campaign(request);
        self.campaigns
            .write()
            .await
            .insert(campaign.id.clone(), campaign.clone());
        Ok(campaign)
    }

    async fn update(
        &self,
        campaign_id: &str,
        request: pb::UpdateCampaignRequest,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        let mut campaigns = self.campaigns.write().await;
        let campaign = campaigns
            .get_mut(campaign_id)
            .ok_or_else(|| RepositoryError::NotFound(campaign_id.to_string()))?;
        if campaign.advertiser_id != request.advertiser_id {
            return Err(RepositoryError::NotFound(campaign_id.to_string()));
        }
        apply_update(campaign, request);
        Ok(campaign.clone())
    }

    async fn get_for_advertiser(
        &self,
        campaign_id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        self.campaigns
            .read()
            .await
            .get(campaign_id)
            .cloned()
            .filter(|campaign| campaign.advertiser_id == advertiser_id)
            .ok_or_else(|| RepositoryError::NotFound(campaign_id.to_string()))
    }

    async fn list_for_advertiser(
        &self,
        advertiser_id: &str,
        limit: usize,
    ) -> Result<Vec<pb::AdCampaign>, RepositoryError> {
        let mut campaigns = self
            .campaigns
            .read()
            .await
            .values()
            .filter(|campaign| campaign.advertiser_id == advertiser_id)
            .cloned()
            .collect::<Vec<_>>();
        campaigns.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        campaigns.truncate(limit);
        Ok(campaigns)
    }

    async fn eligible(
        &self,
        query: EligibleQuery<'_>,
    ) -> Result<Vec<pb::AdCampaign>, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        let day = date_key(now);
        let campaigns = self.campaigns.read().await;
        let events = self.events.read().await;
        let daily_spend = self.daily_spend.read().await;
        let mut values = campaigns
            .values()
            .filter(|campaign| campaign.status == pb::CampaignStatus::Active as i32)
            .filter(|campaign| campaign.placement == query.placement)
            .filter(|campaign| campaign.route_id == query.route_id)
            .filter(|campaign| campaign.action_node_id == query.action_node_id)
            .filter(|campaign| campaign.scene_equipment == query.scene_equipment)
            .filter(|campaign| campaign_starts(campaign).is_none_or(|start| start <= now))
            .filter(|campaign| campaign_ends(campaign).is_none_or(|end| end > now))
            .filter(|campaign| {
                query.domain.is_empty()
                    || campaign.target_domains.is_empty()
                    || campaign
                        .target_domains
                        .iter()
                        .any(|value| value == query.domain)
            })
            .filter_map(|campaign| {
                let mut campaign = campaign.clone();
                campaign.spent_today_micros = *daily_spend
                    .get(&(campaign.id.clone(), day.clone()))
                    .unwrap_or(&0);
                if campaign.daily_budget_micros > 0
                    && campaign.spent_today_micros >= campaign.daily_budget_micros
                {
                    return None;
                }
                let impressions = events
                    .values()
                    .filter(|event| {
                        event.user_id == query.user_id
                            && event.campaign_id == campaign.id
                            && event.event_type == pb::EventType::Impression as i32
                            && date_key(event.occurred_at) == day
                    })
                    .count();
                if campaign.frequency_cap > 0 && impressions >= campaign.frequency_cap as usize {
                    return None;
                }
                let global_impressions = events
                    .values()
                    .filter(|event| {
                        event.campaign_id == campaign.id
                            && event.event_type == pb::EventType::Impression as i32
                            && date_key(event.occurred_at) == day
                    })
                    .count();
                if campaign.global_frequency_cap > 0
                    && global_impressions >= campaign.global_frequency_cap as usize
                {
                    return None;
                }
                Some(campaign)
            })
            .collect::<Vec<_>>();
        values.sort_by(|left, right| {
            right
                .bid_micros
                .cmp(&left.bid_micros)
                .then(left.id.cmp(&right.id))
        });
        values.truncate(query.limit);
        Ok(values)
    }

    async fn register_decisions(
        &self,
        registration: DecisionRegistration<'_>,
    ) -> Result<(), RepositoryError> {
        let campaign_ids = unique_campaign_ids(&registration.campaign_ids)?;
        let campaigns = self.campaigns.read().await;
        for campaign_id in &campaign_ids {
            let campaign = campaigns
                .get(campaign_id)
                .ok_or_else(|| RepositoryError::NotFound(campaign_id.clone()))?;
            if !campaign_matches_registration(campaign, &registration, OffsetDateTime::now_utc()) {
                return Err(RepositoryError::Failed(
                    "campaign does not match the action-node decision context".to_string(),
                ));
            }
        }
        drop(campaigns);
        let expires_at = OffsetDateTime::now_utc() + time::Duration::hours(1);
        let mut decisions = self.decisions.write().await;
        decisions.retain(|_, decision| decision.expires_at > OffsetDateTime::now_utc());
        for campaign_id in campaign_ids {
            let key = (registration.request_id.to_string(), campaign_id);
            if let Some(existing) = decisions.get(&key) {
                if !decision_matches_registration(existing, &registration) {
                    return Err(RepositoryError::Failed(
                        "request id already belongs to a different decision context".to_string(),
                    ));
                }
                continue;
            }
            decisions.insert(
                key,
                MemoryDecision {
                    user_id: registration.user_id.to_string(),
                    placement: registration.placement.to_string(),
                    route_id: registration.route_id.to_string(),
                    action_node_id: registration.action_node_id.to_string(),
                    scene_equipment: registration.scene_equipment.to_string(),
                    expires_at,
                },
            );
        }
        Ok(())
    }

    async fn record_event(
        &self,
        user_id: &str,
        request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        let day = date_key(now);
        let decision = self
            .decisions
            .read()
            .await
            .get(&(request.request_id.clone(), request.campaign_id.clone()))
            .cloned();
        if !decision
            .is_some_and(|decision| decision.user_id == user_id && decision.expires_at > now)
        {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        // All following locks use the same order as candidate reads:
        // campaigns -> events -> daily budget, avoiding a memory-mode deadlock.
        let mut campaigns = self.campaigns.write().await;
        let campaign = campaigns
            .get_mut(&request.campaign_id)
            .ok_or_else(|| RepositoryError::NotFound(request.campaign_id.clone()))?;
        if campaign.status != pb::CampaignStatus::Active as i32 {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        let mut events = self.events.write().await;
        if let Some(existing) = events.get(&request.event_id) {
            if existing.user_id != user_id
                || existing.request_id != request.request_id
                || existing.campaign_id != request.campaign_id
                || existing.event_type != request.event_type
            {
                return Err(RepositoryError::Failed(
                    "event id was already used for a different delivery event".to_string(),
                ));
            }
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: true,
                duplicate: true,
            });
        }
        if events.values().any(|event| {
            event.user_id == user_id
                && event.campaign_id == request.campaign_id
                && event.event_type == request.event_type
                && event.request_id == request.request_id
        }) {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: true,
                duplicate: true,
            });
        }
        let impressions = events
            .values()
            .filter(|event| {
                event.user_id == user_id
                    && event.campaign_id == campaign.id
                    && event.event_type == pb::EventType::Impression as i32
                    && date_key(event.occurred_at) == day
            })
            .count();
        if request.event_type == pb::EventType::Impression as i32
            && campaign.frequency_cap > 0
            && impressions >= campaign.frequency_cap as usize
        {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        let global_impressions = events
            .values()
            .filter(|event| {
                event.campaign_id == campaign.id
                    && event.event_type == pb::EventType::Impression as i32
                    && date_key(event.occurred_at) == day
            })
            .count();
        if request.event_type == pb::EventType::Impression as i32
            && campaign.global_frequency_cap > 0
            && global_impressions >= campaign.global_frequency_cap as usize
        {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        let cost = event_cost(campaign, request.event_type);
        let mut daily_spend = self.daily_spend.write().await;
        let spent = daily_spend.entry((campaign.id.clone(), day)).or_default();
        if campaign.daily_budget_micros > 0
            && spent.saturating_add(cost) > campaign.daily_budget_micros
        {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        *spent = spent.saturating_add(cost);
        campaign.spent_today_micros = *spent;
        if request.event_type == pb::EventType::Impression as i32 {
            campaign.impressions = campaign.impressions.saturating_add(1);
        } else if request.event_type == pb::EventType::Click as i32 {
            campaign.clicks = campaign.clicks.saturating_add(1);
        }
        campaign.updated_at = timestamp(now);
        events.insert(
            request.event_id.clone(),
            MemoryEvent {
                user_id: user_id.to_string(),
                request_id: request.request_id,
                campaign_id: campaign.id.clone(),
                event_type: request.event_type,
                occurred_at: now,
            },
        );
        Ok(pb::EventReceipt {
            event_id: request.event_id,
            accepted: true,
            duplicate: false,
        })
    }
}

#[derive(Clone)]
pub(crate) struct PostgresCampaignRepository {
    pool: PgPool,
}

impl PostgresCampaignRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn get_for_advertiser(
        &self,
        id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        sqlx::query_as::<_, CampaignRow>(&campaign_select(
            "WHERE c.id = $1 AND c.advertiser_id = $2",
        ))
        .bind(id)
        .bind(advertiser_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .map(CampaignRow::into_proto)
        .transpose()?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }
}

#[async_trait]
impl CampaignRepository for PostgresCampaignRepository {
    async fn create(
        &self,
        request: pb::CreateCampaignRequest,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        let campaign = new_campaign(request);
        sqlx::query(
            "INSERT INTO ad_campaigns (id, advertiser_id, name, placement, route_id, action_node_id, scene_equipment, title, body, image_url, landing_url, target_domains, status, pricing_model, bid_micros, daily_budget_micros, frequency_cap, predicted_ctr, predicted_cvr, global_frequency_cap, starts_at, ends_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)",
        )
        .bind(&campaign.id)
        .bind(&campaign.advertiser_id)
        .bind(&campaign.name)
        .bind(&campaign.placement)
        .bind(&campaign.route_id)
        .bind(&campaign.action_node_id)
        .bind(&campaign.scene_equipment)
        .bind(&campaign.title)
        .bind(&campaign.body)
        .bind(&campaign.image_url)
        .bind(&campaign.landing_url)
        .bind(serde_json::to_value(&campaign.target_domains).map_err(|error| RepositoryError::Failed(error.to_string()))?)
        .bind(status_name(campaign.status))
        .bind(pricing_name(campaign.pricing_model))
        .bind(campaign.bid_micros)
        .bind(campaign.daily_budget_micros)
        .bind(i32::try_from(campaign.frequency_cap).unwrap_or(i32::MAX))
        .bind(campaign.predicted_ctr)
        .bind(campaign.predicted_cvr)
        .bind(i32::try_from(campaign.global_frequency_cap).unwrap_or(i32::MAX))
        .bind(parse_timestamp(campaign.starts_at.as_deref())?)
        .bind(parse_timestamp(campaign.ends_at.as_deref())?)
        .execute(&self.pool)
        .await
        .map_err(database)?;
        self.get_for_advertiser(&campaign.id, &campaign.advertiser_id)
            .await
    }

    async fn update(
        &self,
        campaign_id: &str,
        request: pb::UpdateCampaignRequest,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        let status = request.status.map(status_name);
        let targets = request
            .target_domains
            .map(|targets| {
                serde_json::to_value(targets.values)
                    .map_err(|error| RepositoryError::Failed(error.to_string()))
            })
            .transpose()?;
        let updated = sqlx::query(
            "UPDATE ad_campaigns SET status=COALESCE($3,status), name=COALESCE($4,name), title=COALESCE($5,title), body=COALESCE($6,body), image_url=COALESCE($7,image_url), landing_url=COALESCE($8,landing_url), target_domains=COALESCE($9,target_domains), bid_micros=COALESCE($10,bid_micros), daily_budget_micros=COALESCE($11,daily_budget_micros), frequency_cap=COALESCE($12,frequency_cap), starts_at=COALESCE($13,starts_at), ends_at=COALESCE($14,ends_at), predicted_ctr=COALESCE($15,predicted_ctr), predicted_cvr=COALESCE($16,predicted_cvr), global_frequency_cap=COALESCE($17,global_frequency_cap), scene_equipment=COALESCE($18,scene_equipment), updated_at=now() WHERE id=$1 AND advertiser_id=$2",
        )
        .bind(campaign_id)
        .bind(&request.advertiser_id)
        .bind(status)
        .bind(request.name)
        .bind(request.title)
        .bind(request.body)
        .bind(request.image_url)
        .bind(request.landing_url)
        .bind(targets)
        .bind(request.bid_micros)
        .bind(request.daily_budget_micros)
        .bind(request.frequency_cap.map(|value| i32::try_from(value).unwrap_or(i32::MAX)))
        .bind(parse_timestamp(request.starts_at.as_deref())?)
        .bind(parse_timestamp(request.ends_at.as_deref())?)
        .bind(request.predicted_ctr)
        .bind(request.predicted_cvr)
        .bind(request.global_frequency_cap.map(|value| i32::try_from(value).unwrap_or(i32::MAX)))
        .bind(request.scene_equipment)
        .execute(&self.pool)
        .await
        .map_err(database)?
        .rows_affected();
        if updated == 0 {
            return Err(RepositoryError::NotFound(campaign_id.to_string()));
        }
        self.get_for_advertiser(campaign_id, &request.advertiser_id)
            .await
    }

    async fn get_for_advertiser(
        &self,
        campaign_id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, RepositoryError> {
        PostgresCampaignRepository::get_for_advertiser(self, campaign_id, advertiser_id).await
    }

    async fn list_for_advertiser(
        &self,
        advertiser_id: &str,
        limit: usize,
    ) -> Result<Vec<pb::AdCampaign>, RepositoryError> {
        let campaigns = sqlx::query_as::<_, CampaignRow>(&campaign_select(
            "WHERE c.advertiser_id = $1 ORDER BY c.updated_at DESC LIMIT $2",
        ))
        .bind(advertiser_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        campaigns.into_iter().map(CampaignRow::into_proto).collect()
    }

    async fn eligible(
        &self,
        query: EligibleQuery<'_>,
    ) -> Result<Vec<pb::AdCampaign>, RepositoryError> {
        // Apply targeting and both frequency caps in the candidate query. This
        // keeps the auction input bounded under load and prevents campaigns
        // that are already capped from consuming recall/rank capacity.
        let values = sqlx::query_as::<_, CampaignRow>(&campaign_select("WHERE c.status = 'active' AND c.placement = $1 AND c.route_id = $2 AND c.action_node_id = $3 AND c.scene_equipment = $4 AND (c.starts_at IS NULL OR c.starts_at <= now()) AND (c.ends_at IS NULL OR c.ends_at > now()) AND (c.daily_budget_micros = 0 OR COALESCE(stats.spent_micros, 0) < c.daily_budget_micros) AND ($5 = '' OR c.target_domains = '[]'::jsonb OR c.target_domains @> jsonb_build_array($5::text)) AND (c.frequency_cap = 0 OR (SELECT count(*) FROM ad_delivery_events e WHERE e.campaign_id = c.id AND e.user_id = $6 AND e.event_type = 'impression' AND e.occurred_at >= date_trunc('day', now())) < c.frequency_cap) AND (c.global_frequency_cap = 0 OR (SELECT count(*) FROM ad_delivery_events e WHERE e.campaign_id = c.id AND e.event_type = 'impression' AND e.occurred_at >= date_trunc('day', now())) < c.global_frequency_cap) ORDER BY c.bid_micros DESC, c.id LIMIT $7"))
            .bind(query.placement)
            .bind(query.route_id)
            .bind(query.action_node_id)
            .bind(query.scene_equipment)
            .bind(query.domain)
            .bind(query.user_id)
            .bind(i64::try_from(query.limit).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await
            .map_err(database)?;
        values.into_iter().map(CampaignRow::into_proto).collect()
    }

    async fn register_decisions(
        &self,
        registration: DecisionRegistration<'_>,
    ) -> Result<(), RepositoryError> {
        let campaign_ids = unique_campaign_ids(&registration.campaign_ids)?;
        let mut tx = self.pool.begin().await.map_err(database)?;
        let matching_campaigns: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ad_campaigns WHERE id = ANY($1) AND status = 'active' AND placement = $2 AND route_id = $3 AND action_node_id = $4 AND scene_equipment = $5 AND (starts_at IS NULL OR starts_at <= now()) AND (ends_at IS NULL OR ends_at > now())",
        )
        .bind(&campaign_ids)
        .bind(registration.placement)
        .bind(registration.route_id)
        .bind(registration.action_node_id)
        .bind(registration.scene_equipment)
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;
        if matching_campaigns != i64::try_from(campaign_ids.len()).unwrap_or(i64::MAX) {
            return Err(RepositoryError::Failed(
                "campaign does not match the action-node decision context".to_string(),
            ));
        }
        for campaign_id in campaign_ids {
            sqlx::query("INSERT INTO ad_delivery_decisions (request_id,campaign_id,user_id,placement,route_id,action_node_id,scene_equipment,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,now()+interval '1 hour') ON CONFLICT (request_id,campaign_id) DO NOTHING")
                .bind(registration.request_id)
                .bind(campaign_id)
                .bind(registration.user_id)
                .bind(registration.placement)
                .bind(registration.route_id)
                .bind(registration.action_node_id)
                .bind(registration.scene_equipment)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
        }
        let matching_decisions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ad_delivery_decisions WHERE request_id = $1 AND campaign_id = ANY($2) AND user_id = $3 AND placement = $4 AND route_id = $5 AND action_node_id = $6 AND scene_equipment = $7 AND expires_at > now()",
        )
        .bind(registration.request_id)
        .bind(&registration.campaign_ids)
        .bind(registration.user_id)
        .bind(registration.placement)
        .bind(registration.route_id)
        .bind(registration.action_node_id)
        .bind(registration.scene_equipment)
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;
        if matching_decisions != i64::try_from(registration.campaign_ids.len()).unwrap_or(i64::MAX)
        {
            return Err(RepositoryError::Failed(
                "request id already belongs to a different decision context".to_string(),
            ));
        }
        tx.commit().await.map_err(database)?;
        Ok(())
    }

    async fn record_event(
        &self,
        user_id: &str,
        request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let existing_event = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT user_id, request_id, campaign_id, event_type FROM ad_delivery_events WHERE id=$1",
        )
        .bind(&request.event_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if let Some((
            existing_user_id,
            existing_request_id,
            existing_campaign_id,
            existing_event_type,
        )) = existing_event
        {
            if existing_user_id != user_id
                || existing_request_id != request.request_id
                || existing_campaign_id != request.campaign_id
                || existing_event_type != event_name(request.event_type)
            {
                return Err(RepositoryError::Failed(
                    "event id was already used for a different delivery event".to_string(),
                ));
            }
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: true,
                duplicate: true,
            });
        }
        let tracked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ad_delivery_decisions WHERE request_id=$1 AND campaign_id=$2 AND user_id=$3 AND expires_at > now())")
            .bind(&request.request_id)
            .bind(&request.campaign_id)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        let duplicate_decision_event: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ad_delivery_events WHERE request_id=$1 AND campaign_id=$2 AND user_id=$3 AND event_type=$4)")
            .bind(&request.request_id)
            .bind(&request.campaign_id)
            .bind(user_id)
            .bind(event_name(request.event_type))
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        if !tracked {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        if duplicate_decision_event {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: true,
                duplicate: true,
            });
        }
        let row =
            sqlx::query_as::<_, CampaignRow>(&campaign_select("WHERE c.id=$1 FOR UPDATE OF c"))
                .bind(&request.campaign_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(database)?
                .ok_or_else(|| RepositoryError::NotFound(request.campaign_id.clone()))?;
        let mut campaign = row.into_proto()?;
        if campaign.status != pb::CampaignStatus::Active as i32 {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        let day = OffsetDateTime::now_utc().date();
        sqlx::query("INSERT INTO ad_campaign_daily_stats (campaign_id, stat_date) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(&campaign.id)
            .bind(day)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        let (spent,): (i64,) = sqlx::query_as("SELECT spent_micros FROM ad_campaign_daily_stats WHERE campaign_id=$1 AND stat_date=$2 FOR UPDATE")
            .bind(&campaign.id)
            .bind(day)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        let cost = event_cost(&campaign, request.event_type);
        let impressions: i64 = if request.event_type == pb::EventType::Impression as i32 {
            sqlx::query_scalar("SELECT count(*) FROM ad_delivery_events WHERE campaign_id=$1 AND user_id=$2 AND event_type='impression' AND occurred_at >= date_trunc('day', now())")
                .bind(&campaign.id)
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(database)?
        } else {
            0
        };
        if (campaign.frequency_cap > 0
            && request.event_type == pb::EventType::Impression as i32
            && impressions >= i64::from(campaign.frequency_cap))
            || (campaign.daily_budget_micros > 0
                && spent.saturating_add(cost) > campaign.daily_budget_micros)
        {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        if request.event_type == pb::EventType::Impression as i32
            && campaign.global_frequency_cap > 0
        {
            let global_impressions: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM ad_delivery_events WHERE campaign_id=$1 AND event_type='impression' AND occurred_at >= date_trunc('day', now())",
            )
            .bind(&campaign.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if global_impressions >= i64::from(campaign.global_frequency_cap) {
                tx.commit().await.map_err(database)?;
                return Ok(pb::EventReceipt {
                    event_id: request.event_id,
                    accepted: false,
                    duplicate: false,
                });
            }
        }
        sqlx::query("INSERT INTO ad_delivery_events (id, request_id, campaign_id, user_id, event_type, cost_micros) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(&request.event_id)
            .bind(&request.request_id)
            .bind(&campaign.id)
            .bind(user_id)
            .bind(event_name(request.event_type))
            .bind(cost)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        sqlx::query("UPDATE ad_campaign_daily_stats SET spent_micros=spent_micros+$3, impressions=impressions+$4, clicks=clicks+$5 WHERE campaign_id=$1 AND stat_date=$2")
            .bind(&campaign.id)
            .bind(day)
            .bind(cost)
            .bind(if request.event_type == pb::EventType::Impression as i32 { 1_i64 } else { 0 })
            .bind(if request.event_type == pb::EventType::Click as i32 { 1_i64 } else { 0 })
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        sqlx::query("UPDATE ad_campaigns SET impressions=impressions+$2, clicks=clicks+$3, updated_at=now() WHERE id=$1")
            .bind(&campaign.id)
            .bind(if request.event_type == pb::EventType::Impression as i32 { 1_i64 } else { 0 })
            .bind(if request.event_type == pb::EventType::Click as i32 { 1_i64 } else { 0 })
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        tx.commit().await.map_err(database)?;
        campaign.spent_today_micros = spent.saturating_add(cost);
        Ok(pb::EventReceipt {
            event_id: request.event_id,
            accepted: true,
            duplicate: false,
        })
    }
}

#[derive(FromRow)]
struct CampaignRow {
    id: String,
    advertiser_id: String,
    name: String,
    placement: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
    title: String,
    body: String,
    image_url: String,
    landing_url: String,
    target_domains: serde_json::Value,
    status: String,
    pricing_model: String,
    bid_micros: i64,
    daily_budget_micros: i64,
    spent_today_micros: i64,
    frequency_cap: i32,
    predicted_ctr: f64,
    predicted_cvr: f64,
    global_frequency_cap: i32,
    impressions: i64,
    clicks: i64,
    starts_at: Option<OffsetDateTime>,
    ends_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl CampaignRow {
    fn into_proto(self) -> Result<pb::AdCampaign, RepositoryError> {
        Ok(pb::AdCampaign {
            id: self.id,
            advertiser_id: self.advertiser_id,
            name: self.name,
            placement: self.placement,
            route_id: self.route_id,
            action_node_id: self.action_node_id,
            scene_equipment: self.scene_equipment,
            title: self.title,
            body: self.body,
            image_url: self.image_url,
            landing_url: self.landing_url,
            target_domains: serde_json::from_value(self.target_domains)
                .map_err(|error| RepositoryError::Failed(error.to_string()))?,
            status: parse_status(&self.status)? as i32,
            pricing_model: parse_pricing(&self.pricing_model)? as i32,
            bid_micros: self.bid_micros,
            daily_budget_micros: self.daily_budget_micros,
            spent_today_micros: self.spent_today_micros,
            frequency_cap: u32::try_from(self.frequency_cap).unwrap_or_default(),
            predicted_ctr: self.predicted_ctr,
            predicted_cvr: self.predicted_cvr,
            global_frequency_cap: u32::try_from(self.global_frequency_cap).unwrap_or_default(),
            impressions: u64::try_from(self.impressions).unwrap_or_default(),
            clicks: u64::try_from(self.clicks).unwrap_or_default(),
            starts_at: self.starts_at.map(timestamp),
            ends_at: self.ends_at.map(timestamp),
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        })
    }
}

fn campaign_select(where_clause: &str) -> String {
    format!(
        "SELECT c.id,c.advertiser_id,c.name,c.placement,c.route_id,c.action_node_id,c.scene_equipment,c.title,c.body,c.image_url,c.landing_url,c.target_domains,c.status,c.pricing_model,c.bid_micros,c.daily_budget_micros,COALESCE(stats.spent_micros,0) AS spent_today_micros,c.frequency_cap,c.predicted_ctr,c.predicted_cvr,c.global_frequency_cap,c.impressions,c.clicks,c.starts_at,c.ends_at,c.created_at,c.updated_at FROM ad_campaigns c LEFT JOIN ad_campaign_daily_stats stats ON stats.campaign_id=c.id AND stats.stat_date=current_date {where_clause}"
    )
}

fn unique_campaign_ids(campaign_ids: &[String]) -> Result<Vec<String>, RepositoryError> {
    let values = campaign_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if values.len() != campaign_ids.len() {
        return Err(RepositoryError::Failed(
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
fn parse_timestamp(value: Option<&str>) -> Result<Option<OffsetDateTime>, RepositoryError> {
    value
        .map(|value| {
            OffsetDateTime::parse(value, &Rfc3339)
                .map_err(|error| RepositoryError::Failed(error.to_string()))
        })
        .transpose()
}
fn database(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Failed(error.to_string())
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
fn parse_status(value: &str) -> Result<pb::CampaignStatus, RepositoryError> {
    match value {
        "draft" => Ok(pb::CampaignStatus::Draft),
        "active" => Ok(pb::CampaignStatus::Active),
        "paused" => Ok(pb::CampaignStatus::Paused),
        "ended" => Ok(pb::CampaignStatus::Ended),
        _ => Err(RepositoryError::Failed(format!(
            "unknown campaign status {value}"
        ))),
    }
}
fn parse_pricing(value: &str) -> Result<pb::PricingModel, RepositoryError> {
    match value {
        "cpm" => Ok(pb::PricingModel::Cpm),
        "cpc" => Ok(pb::PricingModel::Cpc),
        _ => Err(RepositoryError::Failed(format!(
            "unknown pricing model {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CampaignRepository, DecisionRegistration, MemoryCampaignRepository, RepositoryError,
    };
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
        let repository = MemoryCampaignRepository::default();
        let campaign = repository
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        repository
            .update(
                &campaign.id,
                pb::UpdateCampaignRequest {
                    advertiser_id: campaign.advertiser_id.clone(),
                    status: Some(pb::CampaignStatus::Active as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("campaign should be activated");

        let untracked = repository
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

        repository
            .register_decisions(registration(
                "user-1",
                "request-1",
                vec![campaign.id.clone()],
            ))
            .await
            .expect("decision should be registered");
        let accepted = repository
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

        let duplicate = repository
            .record_event(
                "user-1",
                pb::RecordEventRequest {
                    user_id: "user-1".to_string(),
                    event_id: "event-retry".to_string(),
                    request_id: "request-1".to_string(),
                    campaign_id: campaign.id,
                    event_type: pb::EventType::Impression as i32,
                },
            )
            .await
            .expect("duplicate receipt should be recognized");
        assert!(duplicate.accepted);
        assert!(duplicate.duplicate);
    }

    #[tokio::test]
    async fn global_frequency_cap_blocks_a_second_user() {
        let repository = MemoryCampaignRepository::default();
        let mut request = campaign_request();
        request.global_frequency_cap = 1;
        let campaign = repository
            .create(request)
            .await
            .expect("campaign should be created");
        repository
            .update(
                &campaign.id,
                pb::UpdateCampaignRequest {
                    advertiser_id: campaign.advertiser_id.clone(),
                    status: Some(pb::CampaignStatus::Active as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("campaign should be activated");
        repository
            .register_decisions(registration(
                "user-1",
                "request-1",
                vec![campaign.id.clone()],
            ))
            .await
            .expect("first decision should be tracked");
        assert!(
            repository
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
                .expect("first impression should be accepted")
                .accepted
        );
        repository
            .register_decisions(registration(
                "user-2",
                "request-2",
                vec![campaign.id.clone()],
            ))
            .await
            .expect("second decision should be tracked");
        assert!(
            !repository
                .record_event(
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
        let repository = MemoryCampaignRepository::default();
        let campaign = repository
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        repository
            .update(
                &campaign.id,
                pb::UpdateCampaignRequest {
                    advertiser_id: campaign.advertiser_id.clone(),
                    status: Some(pb::CampaignStatus::Active as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("campaign should be activated");
        repository
            .register_decisions(registration(
                "user-1",
                "request-1",
                vec![campaign.id.clone()],
            ))
            .await
            .expect("first decision should be tracked");
        repository
            .record_event(
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
        repository
            .register_decisions(registration(
                "user-2",
                "request-2",
                vec![campaign.id.clone()],
            ))
            .await
            .expect("second decision should be tracked");
        let conflict = repository
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
            Err(RepositoryError::Failed(message))
                if message == "event id was already used for a different delivery event"
        ));
    }

    #[tokio::test]
    async fn rejects_a_decision_that_does_not_match_the_campaign_action_node() {
        let repository = MemoryCampaignRepository::default();
        let campaign = repository
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        repository
            .update(
                &campaign.id,
                pb::UpdateCampaignRequest {
                    advertiser_id: campaign.advertiser_id.clone(),
                    status: Some(pb::CampaignStatus::Active as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("campaign should be activated");

        let rejected = repository
            .register_decisions(DecisionRegistration {
                action_node_id: "other-node",
                ..registration("user-1", "request-wrong-context", vec![campaign.id.clone()])
            })
            .await;
        assert!(matches!(rejected, Err(RepositoryError::Failed(_))));

        let receipt = repository
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
        let repository = MemoryCampaignRepository::default();
        let campaign = repository
            .create(campaign_request())
            .await
            .expect("campaign should be created");
        repository
            .update(
                &campaign.id,
                pb::UpdateCampaignRequest {
                    advertiser_id: campaign.advertiser_id.clone(),
                    status: Some(pb::CampaignStatus::Active as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("campaign should be activated");

        let rejected = repository
            .register_decisions(DecisionRegistration {
                scene_equipment: "wrong equipment",
                ..registration("user-1", "request-wrong-equipment", vec![campaign.id])
            })
            .await;
        assert!(matches!(rejected, Err(RepositoryError::Failed(_))));
    }

    #[tokio::test]
    async fn advertiser_catalog_is_isolated_for_reads_and_updates() {
        let repository = MemoryCampaignRepository::default();
        let first = repository
            .create(campaign_request())
            .await
            .expect("first advertiser campaign should be created");
        let mut second_request = campaign_request();
        second_request.advertiser_id = "advertiser-2".to_string();
        let second = repository
            .create(second_request)
            .await
            .expect("second advertiser campaign should be created");

        let first_campaigns = repository
            .list_for_advertiser("advertiser-1", 20)
            .await
            .expect("first advertiser list should load");
        assert_eq!(first_campaigns.len(), 1);
        assert_eq!(first_campaigns[0].id, first.id);
        assert!(matches!(
            repository
                .get_for_advertiser(&first.id, "advertiser-2")
                .await,
            Err(RepositoryError::NotFound(_))
        ));
        assert!(matches!(
            repository
                .update(
                    &second.id,
                    pb::UpdateCampaignRequest {
                        advertiser_id: "advertiser-1".to_string(),
                        name: Some("attempted cross-account update".to_string()),
                        ..Default::default()
                    },
                )
                .await,
            Err(RepositoryError::NotFound(_))
        ));
    }
}
