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
                let score = (campaign.bid_micros.max(0) as f64 / 1_000_000.0)
                    + targeting_bonus
                    + (remaining_budget * 0.05);
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
