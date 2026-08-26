use super::*;
use sqlx::{FromRow, PgPool};

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
    fn into_proto(self) -> Result<pb::AdCampaign, DaoError> {
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
                .map_err(|error| DaoError::Failed(error.to_string()))?,
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

#[derive(Clone)]
pub(crate) struct PostgresCampaignDao {
    pool: PgPool,
}

impl PostgresCampaignDao {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn get_for_advertiser(
        &self,
        id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, DaoError> {
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
        .ok_or_else(|| DaoError::NotFound(id.to_string()))
    }
}

#[async_trait]
impl CampaignDao for PostgresCampaignDao {
    async fn create(&self, request: pb::CreateCampaignRequest) -> Result<pb::AdCampaign, DaoError> {
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
        .bind(serde_json::to_value(&campaign.target_domains).map_err(|error| DaoError::Failed(error.to_string()))?)
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
    ) -> Result<pb::AdCampaign, DaoError> {
        let status = request.status.map(status_name);
        let targets = request
            .target_domains
            .map(|targets| {
                serde_json::to_value(targets.values)
                    .map_err(|error| DaoError::Failed(error.to_string()))
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
            return Err(DaoError::NotFound(campaign_id.to_string()));
        }
        self.get_for_advertiser(campaign_id, &request.advertiser_id)
            .await
    }

    async fn get_for_advertiser(
        &self,
        campaign_id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, DaoError> {
        PostgresCampaignDao::get_for_advertiser(self, campaign_id, advertiser_id).await
    }

    async fn list_for_advertiser(
        &self,
        advertiser_id: &str,
        limit: usize,
    ) -> Result<Vec<pb::AdCampaign>, DaoError> {
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

    async fn eligible(&self, query: EligibleQuery<'_>) -> Result<Vec<pb::AdCampaign>, DaoError> {
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
    ) -> Result<(), DaoError> {
        let campaign_ids = unique_campaign_ids(&registration.campaign_ids)?;
        let mut tx = self.pool.begin().await.map_err(database)?;
        // A first request has no decision rows to lock. Serialize by the
        // opaque request id before observing or inserting rows so concurrent
        // retries cannot commit different campaign sets for one request.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(registration.request_id)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        let (existing_total, existing_matching, existing_unexpired): (i64, i64, bool) =
            sqlx::query_as(
                "SELECT count(*), count(*) FILTER (WHERE campaign_id = ANY($2) AND user_id = $3 AND placement = $4 AND route_id = $5 AND action_node_id = $6 AND scene_equipment = $7), COALESCE(bool_and(expires_at > now()), false) FROM ad_delivery_decisions WHERE request_id = $1",
            )
            .bind(registration.request_id)
            .bind(&campaign_ids)
            .bind(registration.user_id)
            .bind(registration.placement)
            .bind(registration.route_id)
            .bind(registration.action_node_id)
            .bind(registration.scene_equipment)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        let expected_decisions = i64::try_from(campaign_ids.len()).unwrap_or(i64::MAX);
        if existing_total > 0 {
            if existing_total == expected_decisions
                && existing_matching == expected_decisions
                && existing_unexpired
            {
                tx.commit().await.map_err(database)?;
                return Ok(());
            }
            return Err(DaoError::Failed(
                "request id already belongs to a different decision context or campaign set"
                    .to_string(),
            ));
        }
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
            return Err(DaoError::Failed(
                "campaign does not match the action-node decision context".to_string(),
            ));
        }
        for campaign_id in &campaign_ids {
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
        .bind(&campaign_ids)
        .bind(registration.user_id)
        .bind(registration.placement)
        .bind(registration.route_id)
        .bind(registration.action_node_id)
        .bind(registration.scene_equipment)
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;
        let total_decisions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ad_delivery_decisions WHERE request_id = $1")
                .bind(registration.request_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(database)?;
        if matching_decisions != expected_decisions || total_decisions != expected_decisions {
            return Err(DaoError::Failed(
                "request id already belongs to a different decision context or campaign set"
                    .to_string(),
            ));
        }
        tx.commit().await.map_err(database)?;
        Ok(())
    }

    async fn record_event(
        &self,
        user_id: &str,
        request: pb::RecordEventRequest,
    ) -> Result<pb::EventReceipt, DaoError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        // The event ID is the transport-level idempotency key. Lock it before
        // the first read so reuse across different campaigns cannot race the
        // primary-key insert while each transaction holds a different row
        // lock.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&request.event_id)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
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
                return Err(DaoError::Failed(
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
        let duplicate_decision_event: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ad_delivery_events WHERE request_id=$1 AND campaign_id=$2 AND user_id=$3 AND event_type=$4)")
            .bind(&request.request_id)
            .bind(&request.campaign_id)
            .bind(user_id)
            .bind(event_name(request.event_type))
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        if duplicate_decision_event {
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
        if !tracked {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        let row =
            sqlx::query_as::<_, CampaignRow>(&campaign_select("WHERE c.id=$1 FOR UPDATE OF c"))
                .bind(&request.campaign_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(database)?
                .ok_or_else(|| DaoError::NotFound(request.campaign_id.clone()))?;
        // Another request with the same event ID may have committed while we
        // waited for the campaign row lock. Recheck after the lock so a retry
        // is idempotent instead of surfacing a unique-key violation.
        if let Some((existing_user_id, existing_request_id, existing_campaign_id, existing_event_type)) =
            sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT user_id, request_id, campaign_id, event_type FROM ad_delivery_events WHERE id=$1",
            )
            .bind(&request.event_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database)?
        {
            if existing_user_id != user_id
                || existing_request_id != request.request_id
                || existing_campaign_id != request.campaign_id
                || existing_event_type != event_name(request.event_type)
            {
                return Err(DaoError::Failed(
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
        let now = OffsetDateTime::now_utc();
        let tracked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ad_delivery_decisions WHERE request_id=$1 AND campaign_id=$2 AND user_id=$3 AND expires_at > $4)",
        )
        .bind(&request.request_id)
        .bind(&request.campaign_id)
        .bind(user_id)
        .bind(now)
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
        // A different event ID for the same decision event may have committed
        // while this transaction waited for the campaign row lock. Recheck
        // the business-level unique key before attempting the insert.
        let duplicate_decision_event: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ad_delivery_events WHERE request_id=$1 AND campaign_id=$2 AND user_id=$3 AND event_type=$4)",
        )
        .bind(&request.request_id)
        .bind(&request.campaign_id)
        .bind(user_id)
        .bind(event_name(request.event_type))
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;
        if duplicate_decision_event {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: true,
                duplicate: true,
            });
        }
        let mut campaign = row.into_proto()?;
        if campaign.status != pb::CampaignStatus::Active as i32
            || campaign_starts(&campaign).is_some_and(|start| start > now)
            || campaign_ends(&campaign).is_some_and(|end| end <= now)
        {
            tx.commit().await.map_err(database)?;
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
            });
        }
        if request.event_type == pb::EventType::Click as i32 {
            let has_accepted_impression: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM ad_delivery_events WHERE request_id=$1 AND campaign_id=$2 AND user_id=$3 AND event_type='impression')",
            )
            .bind(&request.request_id)
            .bind(&campaign.id)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if !has_accepted_impression {
                tx.commit().await.map_err(database)?;
                return Ok(pb::EventReceipt {
                    event_id: request.event_id,
                    accepted: false,
                    duplicate: false,
                });
            }
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
