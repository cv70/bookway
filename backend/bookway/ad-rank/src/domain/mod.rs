use crate::Config;

const MAX_CONTEXT_FIELD_LENGTH: usize = 160;

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
        let route_id = request.route_id.trim();
        let action_node_id = request.action_node_id.trim();
        let scene_equipment = scene_equipment_key(&request.scene_equipment);
        if route_id.is_empty()
            || route_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || action_node_id.is_empty()
            || action_node_id.chars().count() > MAX_CONTEXT_FIELD_LENGTH
            || scene_equipment.is_empty()
            || scene_equipment.chars().count() > MAX_CONTEXT_FIELD_LENGTH
        {
            return crate::api::pb::RankResponse {
                items: Vec::new(),
                model_version: self.config.model_version.clone(),
                degraded: true,
            };
        }
        let mut candidates = request
            .candidates
            .into_iter()
            .filter(|campaign| {
                campaign.route_id == route_id
                    && campaign.action_node_id == action_node_id
                    && scene_equipment_key(&campaign.scene_equipment) == scene_equipment
            })
            .map(|campaign| {
                // Domain affinity is a soft signal: eligibility admits both
                // unrestricted and domain-overlapping campaigns, so the bonus
                // keeps exact-domain relevance ahead. Geo/device need no such
                // term — they are hard eligibility filters in ad-center, so
                // every ranked candidate already matches the request context.
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
                // never changes the amount charged by ad-center. Under
                // `ecpm-v3` the CTR input is calibrated against lifetime
                // delivery evidence instead of trusting the declaration
                // blindly.
                let ecpm = expected_ecpm_micros(&campaign, self.config.calibrated);
                let score = ecpm + targeting_bonus + (remaining_budget * 0.05);
                crate::api::pb::RankedCampaign {
                    campaign: Some(campaign),
                    score,
                    ecpm,
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

fn scene_equipment_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn finite_probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn expected_ecpm_micros(
    campaign: &bookway_ad_center_api::pb::AdCampaign,
    calibrated: bool,
) -> f64 {
    let bid_micros = campaign.bid_micros.max(0) as f64;
    let declared_ctr = finite_probability(campaign.predicted_ctr);
    let ctr = if calibrated {
        serving_ctr(campaign.clicks, campaign.impressions, declared_ctr)
    } else {
        declared_ctr
    };
    let expected_value = bid_micros * ctr * finite_probability(campaign.predicted_cvr);
    match bookway_ad_center_api::pb::PricingModel::try_from(campaign.pricing_model) {
        Ok(bookway_ad_center_api::pb::PricingModel::Cpm) => expected_value,
        Ok(bookway_ad_center_api::pb::PricingModel::Cpc) => expected_value * 1_000.0,
        Err(_) => 0.0,
    }
}

/// Calibrated serving CTR (`ecpm-v3`): the Beta(1, 1) posterior mean over
/// lifetime delivery evidence — `(clicks + 1) / (impressions + 2)` — blended
/// evenly with the advertiser's declared rate, then clamped into
/// `[declared / 2, declared * 2]` so small or noisy samples can never move
/// the serving input beyond a bounded factor of the paid claim. A campaign
/// with no observed impressions keeps its declared rate untouched, so brand
/// new stock behaves exactly like the static model.
///
/// `predicted_cvr` is deliberately not calibrated: the event pipeline records
/// impressions and clicks only — there is no conversion source yet, so any
/// CVR posterior would be invented. When conversion events land upstream this
/// function gains a symmetric twin against the same guardrail shape.
fn serving_ctr(clicks: u64, impressions: u64, declared: f64) -> f64 {
    if impressions == 0 {
        return declared;
    }
    let posterior = (clicks as f64 + 1.0) / (impressions as f64 + 2.0);
    let blended = 0.5 * posterior + 0.5 * declared;
    blended.clamp(declared * 0.5, (declared * 2.0).min(1.0))
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
            calibrated: false,
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
    async fn auction_matches_equipment_with_a_case_insensitive_key() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
            calibrated: false,
        })
        .await
        .expect("domain should initialize");
        let response = domain
            .rank(pb::RankRequest {
                route_id: "route".to_string(),
                action_node_id: "node".to_string(),
                scene_equipment: " Trail Shoes ".to_string(),
                candidates: vec![center::AdCampaign {
                    id: "case-insensitive".to_string(),
                    route_id: "route".to_string(),
                    action_node_id: "node".to_string(),
                    scene_equipment: "trail shoes".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .await;

        assert_eq!(response.items.len(), 1);
        assert_eq!(
            response.items[0]
                .campaign
                .as_ref()
                .map(|campaign| campaign.id.as_str()),
            Some("case-insensitive")
        );
    }

    #[tokio::test]
    async fn auction_normalizes_cpc_to_ecpm_before_mixing_pricing_models() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
            calibrated: false,
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
        assert_eq!(response.items[0].ecpm, 6_000_000.0);
        assert_eq!(response.items[1].ecpm, 5_000_000.0);
    }

    #[tokio::test]
    async fn auction_includes_conversion_quality_in_expected_value() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
            calibrated: false,
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

    #[tokio::test]
    async fn calibration_lifts_truthful_over_delivering_campaigns_within_guardrails() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
            calibrated: true,
        })
        .await
        .expect("domain should initialize");
        // Both campaigns declare the same rate and bid; only delivery history
        // differs. The observed evidence (20% CTR over 10k impressions) wants
        // to push the serving input far above the declaration, but the
        // guardrail caps it at exactly twice the declared rate.
        let response = domain
            .rank(pb::RankRequest {
                user_id: "u".to_string(),
                domain: String::new(),
                route_id: "route".to_string(),
                action_node_id: "node".to_string(),
                scene_equipment: "shoes".to_string(),
                candidates: vec![
                    center::AdCampaign {
                        id: "declared-only".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 1_000_000,
                        predicted_ctr: 0.01,
                        predicted_cvr: 1.0,
                        ..Default::default()
                    },
                    center::AdCampaign {
                        id: "observed-over-delivery".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 1_000_000,
                        predicted_ctr: 0.01,
                        predicted_cvr: 1.0,
                        impressions: 10_000,
                        clicks: 2_000,
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
            "observed-over-delivery"
        );
        // Exactly the clamp ceiling: declared * 2.
        assert_eq!(response.items[0].ecpm, 20_000.0);
        assert_eq!(response.items[1].ecpm, 10_000.0);
    }

    #[tokio::test]
    async fn calibration_disabled_reproduces_the_static_contract() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
            calibrated: false,
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
                        id: "a-declared-only".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 1_000_000,
                        predicted_ctr: 0.01,
                        predicted_cvr: 1.0,
                        impressions: 10_000,
                        clicks: 2_000,
                        ..Default::default()
                    },
                    center::AdCampaign {
                        id: "b-observed-over-delivery".to_string(),
                        route_id: "route".to_string(),
                        action_node_id: "node".to_string(),
                        scene_equipment: "shoes".to_string(),
                        pricing_model: center::PricingModel::Cpm as i32,
                        bid_micros: 1_000_000,
                        predicted_ctr: 0.01,
                        predicted_cvr: 1.0,
                        impressions: 10_000,
                        clicks: 2_000,
                        ..Default::default()
                    },
                ],
            })
            .await;

        // Identical static inputs tie on score and fall back to the stable
        // id ordering — delivery evidence is ignored entirely, which is what
        // the one-key rollback guarantees.
        assert_eq!(
            response.items[0]
                .campaign
                .as_ref()
                .expect("ranked campaign should be present")
                .id,
            "a-declared-only"
        );
        assert!((response.items[0].score - response.items[1].score).abs() < f64::EPSILON);
        assert_eq!(response.items[0].ecpm, 10_000.0);
    }

    #[tokio::test]
    async fn calibration_without_observations_keeps_the_declared_rate() {
        let domain = Domain::new(Config {
            listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            model_version: "test".to_string(),
            calibrated: true,
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
                candidates: vec![center::AdCampaign {
                    id: "brand-new".to_string(),
                    route_id: "route".to_string(),
                    action_node_id: "node".to_string(),
                    scene_equipment: "shoes".to_string(),
                    pricing_model: center::PricingModel::Cpm as i32,
                    bid_micros: 2_500_000,
                    predicted_ctr: 0.04,
                    predicted_cvr: 1.0,
                    ..Default::default()
                }],
            })
            .await;

        assert_eq!(response.items[0].ecpm, 100_000.0);
    }
}
