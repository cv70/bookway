use super::*;

#[derive(Default)]
pub(crate) struct MemoryCampaignDao {
    campaigns: RwLock<HashMap<String, pb::AdCampaign>>,
    events: RwLock<HashMap<String, MemoryEvent>>,
    daily_spend: RwLock<HashMap<(String, String), i64>>,
    decisions: RwLock<HashMap<(String, String), MemoryDecision>>,
}

#[async_trait]
impl CampaignDao for MemoryCampaignDao {
    async fn create(&self, request: pb::CreateCampaignRequest) -> Result<pb::AdCampaign, DaoError> {
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
    ) -> Result<pb::AdCampaign, DaoError> {
        let mut campaigns = self.campaigns.write().await;
        let campaign = campaigns
            .get_mut(campaign_id)
            .ok_or_else(|| DaoError::NotFound(campaign_id.to_string()))?;
        if campaign.advertiser_id != request.advertiser_id {
            return Err(DaoError::NotFound(campaign_id.to_string()));
        }
        apply_update(campaign, request);
        Ok(campaign.clone())
    }

    async fn get_for_advertiser(
        &self,
        campaign_id: &str,
        advertiser_id: &str,
    ) -> Result<pb::AdCampaign, DaoError> {
        self.campaigns
            .read()
            .await
            .get(campaign_id)
            .cloned()
            .filter(|campaign| campaign.advertiser_id == advertiser_id)
            .ok_or_else(|| DaoError::NotFound(campaign_id.to_string()))
    }

    async fn list_for_advertiser(
        &self,
        advertiser_id: &str,
        limit: usize,
    ) -> Result<Vec<pb::AdCampaign>, DaoError> {
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

    async fn eligible(&self, query: EligibleQuery<'_>) -> Result<Vec<pb::AdCampaign>, DaoError> {
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
    ) -> Result<(), DaoError> {
        let campaign_ids = unique_campaign_ids(&registration.campaign_ids)?;
        let campaigns = self.campaigns.read().await;
        for campaign_id in &campaign_ids {
            let campaign = campaigns
                .get(campaign_id)
                .ok_or_else(|| DaoError::NotFound(campaign_id.clone()))?;
            if !campaign_matches_registration(campaign, &registration, OffsetDateTime::now_utc()) {
                return Err(DaoError::Failed(
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
                    return Err(DaoError::Failed(
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
    ) -> Result<pb::EventReceipt, DaoError> {
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
            .ok_or_else(|| DaoError::NotFound(request.campaign_id.clone()))?;
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
                return Err(DaoError::Failed(
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
        if request.event_type == pb::EventType::Click as i32
            && !events.values().any(|event| {
                event.user_id == user_id
                    && event.request_id == request.request_id
                    && event.campaign_id == request.campaign_id
                    && event.event_type == pb::EventType::Impression as i32
            })
        {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: false,
                duplicate: false,
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
