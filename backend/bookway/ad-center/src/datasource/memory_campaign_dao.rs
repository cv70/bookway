use super::*;
use std::collections::BTreeMap;

pub(crate) struct MemoryCampaignDao {
    campaigns: RwLock<HashMap<String, pb::AdCampaign>>,
    events: RwLock<HashMap<String, MemoryEvent>>,
    daily_spend: RwLock<HashMap<(String, String), i64>>,
    decisions: RwLock<HashMap<(String, String), MemoryDecision>>,
    user_daily_total_cap: RwLock<u32>,
}

impl Default for MemoryCampaignDao {
    fn default() -> Self {
        Self {
            campaigns: RwLock::default(),
            events: RwLock::default(),
            daily_spend: RwLock::default(),
            decisions: RwLock::default(),
            // Memory mode has no guardrail table; it starts at the seeded
            // default and `set_user_daily_total_cap` may move it.
            user_daily_total_cap: RwLock::new(DEFAULT_USER_DAILY_TOTAL_CAP),
        }
    }
}

impl MemoryCampaignDao {
    #[cfg(test)]
    pub(crate) async fn expire_decision_for_test(&self, request_id: &str, campaign_id: &str) {
        if let Some(decision) = self
            .decisions
            .write()
            .await
            .get_mut(&(request_id.to_string(), campaign_id.to_string()))
        {
            decision.expires_at = OffsetDateTime::now_utc() - time::Duration::minutes(1);
        }
    }

    /// Shared auction-input filter. With `include_frequency_caps` the two
    /// per-campaign impression caps are adjudicated here; without them this
    /// yields targeting/schedule/budget candidates for the gate pre-filter
    /// path, mirroring the split in the Postgres dao.
    async fn eligible_inner(
        &self,
        query: EligibleQuery<'_>,
        include_frequency_caps: bool,
    ) -> Result<Vec<pb::AdCampaign>, DaoError> {
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
            // Same fail-closed contract as the SQL variant: an empty request
            // region/os never matches restricted campaigns (only unrestricted
            // ones serve without observable delivery context).
            .filter(|campaign| {
                campaign.geo_regions.is_empty()
                    || (!query.geo_region.is_empty()
                        && campaign
                            .geo_regions
                            .iter()
                            .any(|value| value == query.geo_region))
            })
            .filter(|campaign| {
                campaign.device_os.is_empty()
                    || (!query.device_os.is_empty()
                        && campaign.device_os.iter().any(|value| value == query.device_os))
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
                if include_frequency_caps {
                    let impressions = events
                        .values()
                        .filter(|event| {
                            event.user_id == query.user_id
                                && event.campaign_id == campaign.id
                                && event.event_type == pb::EventType::Impression as i32
                                && date_key(event.occurred_at) == day
                        })
                        .count();
                    if campaign.frequency_cap > 0
                        && impressions >= campaign.frequency_cap as usize
                    {
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
        // Full adjudication including both per-campaign impression caps: this
        // is the fail-open path when no Redis gate is configured.
        Self::eligible_inner(self, query, true).await
    }

    async fn eligible_candidates(
        &self,
        query: EligibleQuery<'_>,
    ) -> Result<Vec<pb::AdCampaign>, DaoError> {
        // Targeting/schedule/budget candidates only; frequency adjudication is
        // delegated to the gate pre-filter plus RecordEvent.
        Self::eligible_inner(self, query, false).await
    }

    async fn user_daily_total_cap(&self) -> Result<u32, DaoError> {
        Ok(*self.user_daily_total_cap.read().await)
    }

    async fn set_user_daily_total_cap(&self, cap: u32) -> Result<u32, DaoError> {
        *self.user_daily_total_cap.write().await = cap;
        Ok(cap)
    }

    async fn delivery_report(
        &self,
        query: DeliveryReportQuery<'_>,
    ) -> Result<Vec<DeliveryReportRow>, DaoError> {
        let campaigns = self.campaigns.read().await;
        let events = self.events.read().await;
        let daily_spend = self.daily_spend.read().await;
        let mut rows: Vec<DeliveryReportRow> = Vec::new();
        for campaign in campaigns
            .values()
            .filter(|campaign| campaign.advertiser_id == query.advertiser_id)
        {
            let mut by_day: BTreeMap<String, (i64, i64)> = BTreeMap::new();
            for event in events.values().filter(|event| event.campaign_id == campaign.id) {
                let day = date_key(event.occurred_at);
                if day.as_str() < query.from_date || day.as_str() > query.to_date {
                    continue;
                }
                let entry = by_day.entry(day).or_default();
                if event.event_type == pb::EventType::Impression as i32 {
                    entry.0 += 1;
                } else if event.event_type == pb::EventType::Click as i32 {
                    entry.1 += 1;
                }
            }
            for (campaign_id, day) in daily_spend.keys() {
                if campaign_id != &campaign.id
                    || day.as_str() < query.from_date
                    || day.as_str() > query.to_date
                {
                    continue;
                }
                by_day.entry(day.clone()).or_default();
            }
            for (day, (impressions, clicks)) in by_day {
                rows.push(DeliveryReportRow {
                    campaign_id: campaign.id.clone(),
                    spent_micros: *daily_spend.get(&(campaign.id.clone(), day.clone())).unwrap_or(&0),
                    stat_date: day,
                    impressions,
                    clicks,
                });
            }
        }
        rows.sort_by(|left, right| {
            right
                .stat_date
                .cmp(&left.stat_date)
                .then_with(|| left.campaign_id.cmp(&right.campaign_id))
        });
        Ok(rows)
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
        let existing_campaign_ids = decisions
            .keys()
            .filter(|(request_id, _)| request_id == registration.request_id)
            .map(|(_, campaign_id)| campaign_id.clone())
            .collect::<BTreeSet<_>>();
        let requested_campaign_ids = campaign_ids.iter().cloned().collect::<BTreeSet<_>>();
        if !existing_campaign_ids.is_empty() && existing_campaign_ids != requested_campaign_ids {
            return Err(DaoError::Failed(
                "request id already belongs to a different decision context or campaign set"
                    .to_string(),
            ));
        }
        if !existing_campaign_ids.is_empty()
            && decisions
                .iter()
                .filter(|((request_id, _), _)| request_id == registration.request_id)
                .any(|(_, decision)| decision.expires_at <= OffsetDateTime::now_utc())
        {
            return Err(DaoError::Failed(
                "request id decision lease has expired and cannot be reused".to_string(),
            ));
        }
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
        // Idempotent retries must remain successful even after the decision
        // lease expires or the campaign is paused. Read the event first,
        // then release the lock before taking the campaign lock below.
        if let Some(existing) = self.events.read().await.get(&request.event_id) {
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
        // A retry with a fresh transport event ID is still a duplicate of the
        // already accepted business event, even after the decision lease has
        // expired or the campaign has been paused.
        if self.events.read().await.values().any(|event| {
            event.user_id == user_id
                && event.request_id == request.request_id
                && event.campaign_id == request.campaign_id
                && event.event_type == request.event_type
        }) {
            return Ok(pb::EventReceipt {
                event_id: request.event_id,
                accepted: true,
                duplicate: true,
            });
        }
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
        if campaign.status != pb::CampaignStatus::Active as i32
            || campaign_starts(campaign).is_some_and(|start| start > now)
            || campaign_ends(campaign).is_some_and(|end| end <= now)
        {
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
        // Cross-campaign guardrail, mirroring the Postgres adjudication: even
        // with per-campaign headroom left, a user's impressions today may not
        // pass the platform-wide daily total.
        if request.event_type == pb::EventType::Impression as i32 {
            let total_cap = *self.user_daily_total_cap.read().await;
            if total_cap > 0 {
                let user_total = events
                    .values()
                    .filter(|event| {
                        event.user_id == user_id
                            && event.event_type == pb::EventType::Impression as i32
                            && date_key(event.occurred_at) == day
                    })
                    .count();
                if user_total >= total_cap as usize {
                    return Ok(pb::EventReceipt {
                        event_id: request.event_id,
                        accepted: false,
                        duplicate: false,
                    });
                }
            }
        }
        let cost = event_cost(
            campaign,
            request.event_type,
            i64::try_from(global_impressions).unwrap_or(i64::MAX),
        );
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
