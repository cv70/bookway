use super::*;

/// Redis-side acceleration for ad frequency adjudication.
///
/// Three counter families, one day-scoped key each (counters expire 48h after
/// the first impression of a day):
/// - `adfreq:{campaign}:{user}:{day}` — per-campaign × user impressions,
///   mirroring `AdCampaign.frequency_cap`;
/// - `adgfreq:{campaign}:{day}` — per-campaign global impressions, mirroring
///   `global_frequency_cap`;
/// - `aduday:{user}:{day}` — platform-wide daily impressions per user, the
///   cross-campaign guardrail backed by `ad_delivery_guardrails`.
///
/// The gate is an accelerator, never the authority. Counters drift freely
/// (lost on restart, expired, missed bumps): `RecordEvent` re-adjudicates
/// every accepted impression against Postgres under row locks before anything
/// serves. When Redis cannot answer, callers fall back to the fully
/// SQL-adjudicated `CampaignDao::eligible`, so availability never depends on
/// this component.
#[derive(Clone)]
pub(crate) struct FrequencyGate {
    redis: bookway_data::ConnectionManager,
}

/// Counter lifetime; comfortably past any single delivery day so keys never
/// reset mid-auction, yet stale days age out on their own.
const GATE_TTL_SECONDS: i64 = 48 * 60 * 60;

const FREQUENCY_BUMP_LUA: &str = r#"
local campaign_user = redis.call('INCR', KEYS[1])
if campaign_user == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
local campaign_global = redis.call('INCR', KEYS[2])
if campaign_global == 1 then redis.call('EXPIRE', KEYS[2], ARGV[1]) end
local user_daily_total = redis.call('INCR', KEYS[3])
if user_daily_total == 1 then redis.call('EXPIRE', KEYS[3], ARGV[1]) end
return {campaign_user, campaign_global, user_daily_total}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrequencyCounts {
    campaign_user: i64,
    campaign_global: i64,
    user_daily_total: i64,
}

fn campaign_user_key(campaign_id: &str, user_id: &str, day: &str) -> String {
    format!("adfreq:{campaign_id}:{user_id}:{day}")
}
fn campaign_global_key(campaign_id: &str, day: &str) -> String {
    format!("adgfreq:{campaign_id}:{day}")
}
fn user_daily_key(user_id: &str, day: &str) -> String {
    format!("aduday:{user_id}:{day}")
}

/// Pure cap comparison shared by the pre-filter; unit-tested independently of
/// Redis. Zero means the dimension is unlimited.
fn admits(counts: FrequencyCounts, campaign: &pb::AdCampaign, user_daily_total_cap: u32) -> bool {
    (campaign.frequency_cap == 0
        || counts.campaign_user < i64::from(campaign.frequency_cap))
        && (campaign.global_frequency_cap == 0
            || counts.campaign_global < i64::from(campaign.global_frequency_cap))
        && (user_daily_total_cap == 0
            || counts.user_daily_total < i64::from(user_daily_total_cap))
}

impl FrequencyGate {
    pub(crate) async fn connect() -> Option<Self> {
        match bookway_data::redis_connection().await {
            Ok(Some(redis)) => Some(Self { redis }),
            Ok(None) => {
                tracing::info!(
                    "REDIS_URL unset: ad auction runs SQL-only frequency adjudication"
                );
                None
            }
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "Redis unavailable at startup: ad auction runs SQL-only frequency adjudication"
                );
                None
            }
        }
    }

    /// Drops campaigns whose counters already sit at (or drifted past) a cap.
    /// `None` marks the gate unusable — Redis answered with an error — and the
    /// caller must re-run the SQL-authoritative `eligible`.
    pub(crate) async fn prefilter(
        &self,
        candidates: &[pb::AdCampaign],
        user_id: &str,
        user_daily_total_cap: u32,
    ) -> Option<Vec<pb::AdCampaign>> {
        if candidates.is_empty() {
            return Some(Vec::new());
        }
        let day = date_key(OffsetDateTime::now_utc());
        // Key order mirrors the count mapping below: slot 0 is the user's
        // cross-campaign daily total, then two slots per candidate.
        let mut keys = vec![user_daily_key(user_id, &day)];
        for campaign in candidates {
            keys.push(campaign_user_key(&campaign.id, user_id, &day));
            keys.push(campaign_global_key(&campaign.id, &day));
        }
        let mut redis = self.redis.clone();
        let readings: Vec<Option<i64>> = match redis::cmd("MGET").arg(&keys).query_async(&mut redis).await
        {
            Ok(readings) => readings,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "ad frequency pre-filter degraded; falling back to SQL adjudication"
                );
                return None;
            }
        };
        let count_at = |slot: usize| readings.get(slot).copied().flatten().unwrap_or(0);
        let user_daily_total = count_at(0);
        let admitted = candidates
            .iter()
            .enumerate()
            .filter(|(index, campaign)| {
                let base = 1 + 2 * index;
                let counts = FrequencyCounts {
                    campaign_user: count_at(base),
                    campaign_global: count_at(base + 1),
                    user_daily_total,
                };
                admits(counts, campaign, user_daily_total_cap)
            })
            .map(|(_, campaign)| campaign.clone())
            .collect();
        Some(admitted)
    }

    /// Counts one accepted impression across all three families atomically.
    /// Best-effort by contract: the authoritative ledger was already written
    /// in RecordEvent, so a failed bump here only loosens the next pre-filter
    /// until Postgres re-adjudicates.
    pub(crate) async fn record_impression(&self, user_id: &str, campaign_id: &str) {
        let day = date_key(OffsetDateTime::now_utc());
        let script = redis::Script::new(FREQUENCY_BUMP_LUA);
        let mut invocation = script.prepare_invoke();
        invocation.key(campaign_user_key(campaign_id, user_id, &day));
        invocation.key(campaign_global_key(campaign_id, &day));
        invocation.key(user_daily_key(user_id, &day));
        invocation.arg(GATE_TTL_SECONDS);
        let mut redis = self.redis.clone();
        let result: Result<(i64, i64, i64), _> = invocation.invoke_async(&mut redis).await;
        if let Err(error) = result {
            tracing::warn!(
                error = ?error,
                campaign_id,
                "ad frequency counter bump degraded; counters may lag the ledger"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrequencyCounts, admits};
    use crate::api::pb;

    fn campaign(frequency_cap: u32, global_frequency_cap: u32) -> pb::AdCampaign {
        pb::AdCampaign {
            id: "campaign-1".to_string(),
            frequency_cap,
            global_frequency_cap,
            ..Default::default()
        }
    }

    #[test]
    fn zero_caps_never_filter_any_counter_level() {
        let counts = FrequencyCounts {
            campaign_user: 999,
            campaign_global: 999,
            user_daily_total: 999,
        };
        assert!(admits(counts, &campaign(0, 0), 0));
    }

    #[test]
    fn per_campaign_user_cap_rejects_when_reached() {
        let counts = FrequencyCounts {
            campaign_user: 3,
            campaign_global: 0,
            user_daily_total: 0,
        };
        assert!(!admits(counts, &campaign(3, 0), 0));
        assert!(admits(
            FrequencyCounts { campaign_user: 2, ..counts },
            &campaign(3, 0),
            0
        ));
    }

    #[test]
    fn cross_campaign_daily_total_cap_is_enforced_independently_of_per_campaign_headroom() {
        let counts = FrequencyCounts {
            campaign_user: 0,
            campaign_global: 0,
            user_daily_total: 8,
        };
        assert!(!admits(counts, &campaign(15, 24), 8));
    }
}
