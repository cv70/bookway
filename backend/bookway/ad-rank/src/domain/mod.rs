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
            .filter(|campaign| {
                campaign.route_id == request.route_id
                    && campaign.action_node_id == request.action_node_id
                    && campaign.scene_equipment == request.scene_equipment
            })
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
                // Rank on the versioned expected-value contract shared with
                // campaign evaluation: bid * pCTR * pCVR. CPC bids are
                // normalized to an impression-equivalent value before they
                // are mixed with CPM bids; pCVR remains a quality signal and
                // never changes the amount charged by ad-center.
                let ecpm = expected_ecpm_micros(&campaign);
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
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn expected_ecpm_micros(campaign: &bookway_ad_center_api::pb::AdCampaign) -> f64 {
    let bid_micros = campaign.bid_micros.max(0) as f64;
    let expected_value = bid_micros
        * finite_probability(campaign.predicted_ctr)
        * finite_probability(campaign.predicted_cvr);
    match bookway_ad_center_api::pb::PricingModel::try_from(campaign.pricing_model) {
        Ok(bookway_ad_center_api::pb::PricingModel::Cpm) => expected_value,
        Ok(bookway_ad_center_api::pb::PricingModel::Cpc) => expected_value * 1_000.0,
        Err(_) => 0.0,
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
                scene_equipment: "shoes".to_string(),
                candidates: vec![
                    center::AdCampaign {
                        id: "high-bid-low-value".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        bid_micros: 1_000_000,
                        pricing_model: center::PricingModel::Cpc as i32,
                        predicted_ctr: 0.01,
                        predicted_cvr: 1.0,
                        ..Default::default()
                    },
                    center::AdCampaign {
                        id: "lower-bid-high-value".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        bid_micros: 500_000,
                        pricing_model: center::PricingModel::Cpc as i32,
                        predicted_ctr: 0.8,
                        predicted_cvr: 1.0,
                        ..Default::default()
                    },
                ],
            })
            .await;
        assert_eq!(
            response.items[0]
                .campaign
                .as_ref()
                .expect("ranked campaign should be present")
                .id,
            "lower-bid-high-value"
        );
    }

    #[tokio::test]
    async fn auction_normalizes_cpc_to_ecpm_before_mixing_pricing_models() {
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
                scene_equipment: "shoes".to_string(),
                candidates: vec![
                    center::AdCampaign {
                        id: "cpm".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 6_000_000,
                        predicted_ctr: 1.0,
                        predicted_cvr: 1.0,
                        ..Default::default()
                    },
                    center::AdCampaign {
                        id: "cpc".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpc as i32,
                        bid_micros: 500_000,
                        predicted_ctr: 0.01,
                        predicted_cvr: 1.0,
                        ..Default::default()
                    },
                ],
            })
            .await;

        assert_eq!(
            response.items[0]
                .campaign
                .as_ref()
                .expect("ranked campaign should be present")
                .id,
            "cpm"
        );
        assert!((response.items[0].score - 6_000_000.2).abs() < f64::EPSILON);
        assert!((response.items[1].score - 5_000_000.2).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn auction_includes_conversion_quality_in_expected_value() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
        })
        .await
        .expect("domain should initialize");
        let response = domain
            .rank(pb::RankRequest {
                user_id: "u".to_string(),
                domain: String::new(),
                route_id: "route".to_string(),
                action_node_id: "node".to_string(),
                scene_equipment: "shoes".to_string(),
                candidates: vec![
                    center::AdCampaign {
                        id: "high-bid-low-cvr".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 2_000_000,
                        predicted_ctr: 0.8,
                        predicted_cvr: 0.01,
                        ..Default::default()
                    },
                    center::AdCampaign {
                        id: "lower-bid-high-cvr".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 1_000_000,
                        predicted_ctr: 0.8,
                        predicted_cvr: 0.8,
                        ..Default::default()
                    },
                ],
            })
            .await;
        assert_eq!(
            response.items[0]
                .campaign
                .as_ref()
                .expect("ranked campaign should be present")
                .id,
            "lower-bid-high-cvr"
        );
    }
}
