use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use bookway_data::ConnectionManager;
use redis::AsyncCommands;
use thiserror::Error;
use time::OffsetDateTime;

/// Per-user, per-content daily exposure hard cap. Unlike the softer
/// `previously_served` fatigue signal (all-time history), this counter is what
/// actually removes an item from today's slate once it crossed its allowance.
///
/// The key layout and TTL are shared by both directions: hydrators read these
/// counters before ranking, and the exposure side effect increments them after
/// a response is served.
const KEY_PREFIX: &str = "fcap";
/// Keys outlive the UTC day by 24h so late-evening traffic still reads the
/// previous day's counters during clock skew, then self-expires.
const KEY_TTL_SECS: u64 = 48 * 60 * 60;

#[derive(Debug, Error)]
pub(crate) enum FrequencyCapError {
    #[error("redis operation failed: {0}")]
    Redis(#[from] redis::RedisError),
}

#[async_trait]
pub(crate) trait FrequencyCapDataSource: Send + Sync {
    /// Served counts for each content id, positionally aligned with the input.
    /// An error means the guard could not be evaluated — callers fail open.
    async fn served_counts(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> Result<Vec<u32>, FrequencyCapError>;

    /// Atomically bumps each content's today-counter (first increment of the
    /// day installs the 48h TTL).
    async fn record_served(&self, user_id: &str, content_ids: &[String])
        -> Result<(), FrequencyCapError>;
}

/// For deployments without a backing store (no `REDIS_URL`) or with the guard
/// disabled via config. Reads always report zero so every item stays eligible;
/// writes are accepted and dropped. Chosen explicitly at wiring time and
/// logged once at startup — never silently swapped at runtime.
#[derive(Debug, Default)]
pub(crate) struct DisabledFrequencyCapDataSource;

#[async_trait]
impl FrequencyCapDataSource for DisabledFrequencyCapDataSource {
    async fn served_counts(
        &self,
        _user_id: &str,
        content_ids: &[String],
    ) -> Result<Vec<u32>, FrequencyCapError> {
        Ok(content_ids.iter().map(|_| 0).collect())
    }

    async fn record_served(
        &self,
        _user_id: &str,
        _content_ids: &[String],
    ) -> Result<(), FrequencyCapError> {
        Ok(())
    }
}

pub(crate) fn daily_key(user_id: &str, content_id: &str, date: time::Date) -> String {
    format!("{KEY_PREFIX}:{user_id}:{content_id}:{}", day_stamp(date))
}

fn day_stamp(date: time::Date) -> String {
    format!(
        "{:04}{:02}{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

fn today() -> time::Date {
    OffsetDateTime::now_utc().date()
}

#[derive(Debug, Default)]
pub(crate) struct MemoryFrequencyCapDataSource {
    counts: Mutex<HashMap<String, u32>>,
}

#[async_trait]
impl FrequencyCapDataSource for MemoryFrequencyCapDataSource {
    async fn served_counts(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> Result<Vec<u32>, FrequencyCapError> {
        let date = today();
        let counts = self.counts.lock().expect("frequency-cap lock poisoned");
        Ok(content_ids
            .iter()
            .map(|content_id| {
                counts
                    .get(&daily_key(user_id, content_id, date))
                    .copied()
                    .unwrap_or_default()
            })
            .collect())
    }

    async fn record_served(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> Result<(), FrequencyCapError> {
        let date = today();
        let mut counts = self.counts.lock().expect("frequency-cap lock poisoned");
        for content_id in content_ids {
            counts
                .entry(daily_key(user_id, content_id, date))
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }
        Ok(())
    }
}

/// Production store. Read side is one pipelined MGET; write side is a single
/// Lua evaluation so INCR and the first-day EXPIRE stay atomic per batch.
#[derive(Clone)]
pub(crate) struct RedisFrequencyCapDataSource {
    redis: ConnectionManager,
}

// The multi-key script keeps N writes to one round trip; `v == 1` marks the
// counter created today, which is when the 48h TTL must be installed so
// repeated serving within the day never extends the window.
const RECORD_SERVED_LUA: &str = r"
for _, key in ipairs(KEYS) do
    local v = redis.call('INCR', key)
    if v == 1 then
        redis.call('EXPIRE', key, ARGV[1])
    end
end
return redis.status_reply('OK')
";

impl RedisFrequencyCapDataSource {
    pub(crate) fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl FrequencyCapDataSource for RedisFrequencyCapDataSource {
    async fn served_counts(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> Result<Vec<u32>, FrequencyCapError> {
        if content_ids.is_empty() {
            return Ok(Vec::new());
        }
        let date = today();
        let keys: Vec<String> = content_ids
            .iter()
            .map(|content_id| daily_key(user_id, content_id, date))
            .collect();
        let raw: Vec<Option<i64>> = self
            .redis
            .clone()
            .mget(&keys)
            .await
            .map_err(FrequencyCapError::from)?;
        Ok(raw
            .into_iter()
            .map(|value| value.and_then(|count| u32::try_from(count).ok()).unwrap_or(0))
            .collect())
    }

    async fn record_served(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> Result<(), FrequencyCapError> {
        use redis::Script;

        let date = today();
        let keys: Vec<String> = content_ids
            .iter()
            .map(|content_id| daily_key(user_id, content_id, date))
            .collect();
        Script::new(RECORD_SERVED_LUA)
            .key(keys)
            .arg(KEY_TTL_SECS)
            .invoke_async(&mut self.redis.clone())
            .await
            .map_err(FrequencyCapError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FrequencyCapDataSource, MemoryFrequencyCapDataSource, daily_key, day_stamp,
    };
    use time::{Date, Month};

    #[test]
    fn day_stamp_is_zero_padded_yyyymmdd() {
        let date = Date::from_calendar_date(2026, Month::February, 5).expect("valid date");
        assert_eq!(day_stamp(date), "20260205");
        let date = Date::from_calendar_date(2026, Month::November, 23).expect("valid date");
        assert_eq!(day_stamp(date), "20261123");
    }

    #[test]
    fn keys_scoped_per_user_per_content_per_day() {
        let date = Date::from_calendar_date(2026, Month::August, 27).expect("valid date");
        assert_eq!(
            daily_key("user-1", "post-9", date),
            "fcap:user-1:post-9:20260827"
        );
    }

    #[tokio::test]
    async fn memory_store_counts_accumulate_and_stay_user_scoped() {
        let store = MemoryFrequencyCapDataSource::default();
        let posts = ["post-1".to_string(), "post-2".to_string()];
        for _ in 0..3 {
            store.record_served("user-1", &posts).await.expect("record");
        }
        store
            .record_served("user-2", std::slice::from_ref(&posts[0]))
            .await
            .expect("record");

        assert_eq!(
            store.served_counts("user-1", &posts).await.expect("memory counts"),
            vec![3, 3]
        );
        // Another user's ledger is untouched; unknown content starts at zero.
        assert_eq!(
            store.served_counts("user-2", &posts).await.expect("memory counts"),
            vec![1, 0]
        );
        assert_eq!(
            store
                .served_counts("user-2", &["never-served".to_string()])
                .await
                .expect("memory counts"),
            vec![0]
        );
    }

    #[tokio::test]
    async fn empty_batch_short_circuits_without_touching_the_store() {
        let store = MemoryFrequencyCapDataSource::default();
        assert!(store
            .served_counts("user-1", &[])
            .await
            .expect("memory counts")
            .is_empty());
        store
            .record_served("user-1", &[])
            .await
            .expect("empty record is a no-op");
    }
}
