use crate::Config;
#[derive(Clone)]
pub struct Domain {
    config: Config,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, std::convert::Infallible> {
        Ok(Self { config })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn rank(
        &self,
        request: crate::api::pb::RankRequest,
    ) -> crate::api::pb::RankResponse {
        let mut candidates = request
            .candidates
            .into_iter()
            .map(|campaign| {
                let targeting_bonus = if request.domain.is_empty()
                    || campaign.target_domains.is_empty()
                    || campaign
                        .target_domains
                        .iter()
                        .any(|domain| domain == &request.domain)
                {
                    0.15
                } else {
                    0.0
                };
                let remaining_budget = if campaign.daily_budget_micros > 0 {
                    (campaign.daily_budget_micros - campaign.spent_today_micros).max(0) as f64
                        / campaign.daily_budget_micros as f64
                } else {
                    1.0
                };
                // Expected value auction: bid * pCTR * pCVR, with a small
                // prior for campaigns that have no model prediction yet.
                let predicted_ctr = finite_probability(campaign.predicted_ctr);
                let predicted_cvr = finite_probability(campaign.predicted_cvr);
                let ecpm = (campaign.bid_micros.max(0) as f64 / 1_000_000.0)
                    * predicted_ctr
                    * predicted_cvr;
                let score = ecpm + targeting_bonus + (remaining_budget * 0.05);
                crate::api::pb::RankedCampaign {
                    campaign: Some(campaign),
                    score,
                }
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right.score.total_cmp(&left.score).then_with(|| {
                left.campaign
                    .as_ref()
                    .map(|campaign| &campaign.id)
                    .cmp(&right.campaign.as_ref().map(|campaign| &campaign.id))
            })
        });
        crate::api::pb::RankResponse {
            items: candidates,
            model_version: self.config.model_version.clone(),
            degraded: false,
        }
    }
}

fn finite_probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0001, 1.0)
    } else {
        0.0001
    }
}

#[cfg(test)]
mod tests {
    use super::Domain;
    use crate::Config;
    use bookway_ad_center_api::pb as center;
    use bookway_ad_rank_api::pb;

    #[tokio::test]
    async fn auction_prefers_expected_value_over_bid_alone() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
        })
        .await
        .expect("domain should initialize");
        let response = domain
            .rank(pb::RankRequest {
                user_id: "u".to_string(),
                domain: "travel".to_string(),
                route_id: "route".to_string(),
                action_node_id: "node".to_string(),
                candidates: vec![
                    center::AdCampaign {
                        id: "high-bid-low-value".to_string(),
                        bid_micros: 1_000_000,
                        predicted_ctr: 0.01,
                        predicted_cvr: 0.01,
                        ..Default::default()
                    },
                    center::AdCampaign {
                        id: "lower-bid-high-value".to_string(),
                        bid_micros: 500_000,
                        predicted_ctr: 0.8,
                        predicted_cvr: 0.8,
                        ..Default::default()
                    },
                ],
            })
            .await;
        assert_eq!(
            response.items[0].campaign.as_ref().unwrap().id,
            "lower-bid-high-value"
        );
    }
}
