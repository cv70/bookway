//! Optional per-user minimum interval between served ad decisions.
//!
//! A pure serving-experience throttle configured by the operator — not a
//! delivery guarantee like the frequency caps that live in ad-center. The
//! timestamp keys are best effort: they may drift, they expire by TTL, and a
//! Redis outage simply turns pacing off (fail-open) instead of blocking
//! commerce.

use bookway_data::ConnectionManager;
use std::time::Duration;

const PACING_KEY_PREFIX: &str = "adpace:";

/// User-level decision cooldown marker. Exists while the user is within
/// their configured minimum impression interval.
#[derive(Clone)]
pub(crate) struct ImpressionPacing {
    redis: ConnectionManager,
    ttl_seconds: i64,
}

fn pacing_ttl_seconds(cooldown: Duration) -> i64 {
    i64::try_from(u64::try_from(cooldown.as_millis().max(1)).unwrap_or(u64::MAX) / 1000)
        .unwrap_or(1)
        .max(1)
}

impl ImpressionPacing {
    /// Builds the gate only when an interval is configured AND Redis is
    /// reachable; otherwise pacing silently stays off.
    pub(crate) async fn connect(cooldown: Option<Duration>) -> Option<Self> {
        let cooldown = cooldown?;
        match bookway_data::redis_connection().await {
            Ok(Some(connection)) => Some(Self {
                redis: connection,
                ttl_seconds: pacing_ttl_seconds(cooldown),
            }),
            Ok(None) => {
                tracing::info!("REDIS_URL unset; impression pacing stays disabled");
                None
            }
            Err(error) => {
                tracing::warn!(error = ?error, "Redis unavailable; impression pacing stays disabled");
                None
            }
        }
    }

    /// True when the user received ad decisions inside the current window.
    pub(crate) async fn cooling_down(&self, user_id: &str) -> bool {
        let mut connection = self.redis.clone();
        let result = redis::cmd("GET")
            .arg(format!("{PACING_KEY_PREFIX}{user_id}"))
            .query_async::<Option<String>>(&mut connection)
            .await;
        match result {
            Ok(existing) => existing.is_some(),
            Err(error) => {
                tracing::warn!(error = ?error, "impression pacing probe failed; serving normally");
                false
            }
        }
    }

    /// Arms the cooldown after a decision actually carried ads.
    pub(crate) async fn mark_served(&self, user_id: &str) {
        let mut connection = self.redis.clone();
        let result = redis::cmd("SET")
            .arg(format!("{PACING_KEY_PREFIX}{user_id}"))
            .arg("1")
            .arg("EX")
            .arg(self.ttl_seconds)
            .query_async::<Option<String>>(&mut connection)
            .await;
        if let Err(error) = result {
            tracing::warn!(error = ?error, "impression pacing write failed; window may be shortened");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pacing_ttl_seconds;
    use std::time::Duration;

    #[test]
    fn sub_second_intervals_still_get_a_positive_ttl() {
        assert_eq!(pacing_ttl_seconds(Duration::from_millis(0)), 1);
        assert_eq!(pacing_ttl_seconds(Duration::from_millis(250)), 1);
        assert_eq!(pacing_ttl_seconds(Duration::from_secs(90)), 90);
    }
}
