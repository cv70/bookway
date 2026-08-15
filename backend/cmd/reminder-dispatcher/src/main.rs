use std::{env, time::Duration};

use sqlx::PgPool;
use thiserror::Error;

#[derive(Debug, Error)]
enum DispatcherError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
}

struct Config {
    batch_size: i64,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            batch_size: env_number("REMINDER_DISPATCH_BATCH_SIZE", 100_i64)?.clamp(1, 1_000),
        })
    }
}

struct ReminderDispatcher {
    pool: PgPool,
    batch_size: i64,
}

impl ReminderDispatcher {
    fn new(pool: PgPool, batch_size: i64) -> Self {
        Self { pool, batch_size }
    }

    async fn run_once(&self) -> Result<u64, DispatcherError> {
        // Lock actions while materializing the durable delivery and its outbox command.
        // A provider consumer must re-check the queued delivery before sending, because a
        // completion or reschedule can cancel it after the outbox event is published.
        let result = sqlx::query(
            r#"WITH candidates AS MATERIALIZED (
                SELECT a.id AS action_id, a.user_id, a.schedule_revision, a.scheduled_at,
                       a.payload ->> 'title' AS action_title, d.device_id
                FROM actions a
                JOIN journeys j
                  ON j.id = a.journey_id AND j.user_id = a.user_id AND j.status = 'active'
                JOIN reminder_preferences p ON p.user_id = a.user_id
                JOIN push_devices d ON d.user_id = a.user_id AND d.active
                WHERE a.state = 'pending'
                  AND a.scheduled_at IS NOT NULL
                  AND p.enabled
                  AND a.scheduled_at <= now() + make_interval(mins => p.lead_minutes::integer)
                  AND (
                    p.quiet_hours_start IS NULL
                    OR CASE WHEN p.quiet_hours_start < p.quiet_hours_end
                      THEN (now() AT TIME ZONE p.timezone)::time < p.quiet_hours_start
                        OR (now() AT TIME ZONE p.timezone)::time >= p.quiet_hours_end
                      ELSE (now() AT TIME ZONE p.timezone)::time >= p.quiet_hours_end
                        AND (now() AT TIME ZONE p.timezone)::time < p.quiet_hours_start
                    END
                  )
                ORDER BY a.scheduled_at, a.id, d.device_id
                FOR UPDATE OF a SKIP LOCKED
                LIMIT $1
            ), deliveries AS (
                INSERT INTO reminder_deliveries
                    (user_id, action_id, device_id, channel, schedule_revision, scheduled_at)
                SELECT user_id, action_id, device_id, 'push', schedule_revision, scheduled_at
                FROM candidates
                ON CONFLICT (action_id, schedule_revision, channel, device_id) DO NOTHING
                RETURNING id, user_id, action_id, device_id, schedule_revision, scheduled_at
            ), notices AS (
                -- An action may be queued to several devices, but its inbox item is singular.
                INSERT INTO user_notifications
                    (user_id, kind, source_id, title, body, data)
                SELECT d.user_id,
                       'action_reminder',
                       CONCAT(d.action_id, ':', d.schedule_revision),
                       '行动提醒',
                       CONCAT('“', COALESCE(NULLIF(c.action_title, ''), '已安排的行动'), '” 即将开始，准备好时从一个最小步骤开始。'),
                       jsonb_build_object(
                         'action_id', d.action_id,
                         'schedule_revision', d.schedule_revision,
                         'scheduled_at', d.scheduled_at
                       )
                FROM deliveries d
                JOIN candidates c
                  ON c.action_id = d.action_id
                 AND c.device_id = d.device_id
                 AND c.schedule_revision = d.schedule_revision
                ON CONFLICT (kind, source_id) DO NOTHING
            )
            INSERT INTO outbox_events (aggregate_type, aggregate_id, event_type, payload)
            SELECT 'reminder_delivery', d.id::text, 'notification.reminder.requested.v1',
                   jsonb_build_object(
                     'delivery_id', d.id,
                     'user_id', d.user_id,
                     'device_id', d.device_id,
                     'action_id', d.action_id,
                     'schedule_revision', d.schedule_revision,
                     'scheduled_at', d.scheduled_at
                   )
            FROM deliveries d"#,
        )
        .bind(self.batch_size)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

fn env_number<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|_| ConfigError::Invalid { key, value }),
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("reminder-dispatcher");
    let config = Config::from_env()?;
    let dispatcher =
        ReminderDispatcher::new(bookway_data::postgres_pool().await?, config.batch_size);
    loop {
        match dispatcher.run_once().await {
            Ok(0) => tokio::time::sleep(Duration::from_millis(500)).await,
            Ok(count) => tracing::debug!(count, "reminder delivery commands queued"),
            Err(error) => {
                tracing::error!(%error, "reminder dispatcher iteration failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
