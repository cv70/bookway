use std::env;

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 10_000;
const DEFAULT_MIN_DEAD_AGE_SECONDS: i32 = 300;
const MAX_MIN_DEAD_AGE_SECONDS: i32 = 30 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryAction {
    Report,
    RequeueDead,
}

impl RecoveryAction {
    fn from_env_value(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "report" => Ok(Self::Report),
            "requeue_dead" => Ok(Self::RequeueDead),
            _ => Err(ConfigError::Invalid {
                key: "SEARCH_INDEX_RECOVERY_ACTION",
                value: value.to_string(),
            }),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Report => "report",
            Self::RequeueDead => "requeue_dead",
        }
    }
}

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
    #[error("{key} is required when SEARCH_INDEX_RECOVERY_ACTION=requeue_dead")]
    MissingApproval { key: &'static str },
}

#[derive(Debug, Error)]
enum RecoveryError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug)]
struct Config {
    action: RecoveryAction,
    limit: i64,
    min_dead_age_seconds: i32,
    actor: Option<String>,
    reason: Option<String>,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        let action = env::var("SEARCH_INDEX_RECOVERY_ACTION")
            .map(|value| RecoveryAction::from_env_value(&value))
            .unwrap_or(Ok(RecoveryAction::Report))?;
        let actor = optional_bounded_env("SEARCH_INDEX_RECOVERY_ACTOR", 128)?;
        let reason = optional_bounded_env("SEARCH_INDEX_RECOVERY_REASON", 500)?;
        validate_approval(action, actor.as_deref(), reason.as_deref())?;
        Ok(Self {
            action,
            limit: env_number("SEARCH_INDEX_RECOVERY_LIMIT", DEFAULT_LIMIT)?.clamp(1, MAX_LIMIT),
            min_dead_age_seconds: env_number(
                "SEARCH_INDEX_RECOVERY_MIN_DEAD_AGE_SECONDS",
                DEFAULT_MIN_DEAD_AGE_SECONDS,
            )?
            .clamp(0, MAX_MIN_DEAD_AGE_SECONDS),
            actor,
            reason,
        })
    }
}

fn validate_approval(
    action: RecoveryAction,
    actor: Option<&str>,
    reason: Option<&str>,
) -> Result<(), ConfigError> {
    if matches!(action, RecoveryAction::RequeueDead) {
        if actor.is_none() {
            return Err(ConfigError::MissingApproval {
                key: "SEARCH_INDEX_RECOVERY_ACTOR",
            });
        }
        if reason.is_none() {
            return Err(ConfigError::MissingApproval {
                key: "SEARCH_INDEX_RECOVERY_REASON",
            });
        }
    }
    Ok(())
}

fn optional_bounded_env(
    key: &'static str,
    max_characters: usize,
) -> Result<Option<String>, ConfigError> {
    let value = env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > max_characters)
    {
        return Err(ConfigError::Invalid {
            key,
            value: format!("exceeds {max_characters} characters"),
        });
    }
    Ok(value)
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

#[derive(Debug, Serialize, sqlx::FromRow)]
struct OutboxStatus {
    status: String,
    count: i64,
    max_attempts: i32,
    oldest_age_seconds: i64,
}

#[derive(Debug, Serialize)]
struct RecoveryResult {
    run_id: String,
    action: RecoveryAction,
    recovered_count: i64,
    before: Vec<OutboxStatus>,
    after: Vec<OutboxStatus>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-index-outbox-recovery");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let run_id = Uuid::now_v7();
    let (recovered_count, before, after) = match config.action {
        RecoveryAction::Report => record_report(&pool, run_id, &config).await?,
        RecoveryAction::RequeueDead => requeue_dead(&pool, run_id, &config).await?,
    };
    let result = RecoveryResult {
        run_id: run_id.to_string(),
        action: config.action,
        recovered_count,
        before,
        after,
    };
    tracing::info!(
        run_id = %result.run_id,
        action = result.action.as_str(),
        recovered_count = result.recovered_count,
        "search index outbox recovery run completed"
    );
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

async fn record_report(
    pool: &sqlx::PgPool,
    run_id: Uuid,
    config: &Config,
) -> Result<(i64, Vec<OutboxStatus>, Vec<OutboxStatus>), RecoveryError> {
    let before = load_summary(pool).await?;
    sqlx::query(
        "INSERT INTO content_index_recovery_runs (id,action,actor,reason,requested_limit,min_dead_age_seconds,recovered_count,completed_at) VALUES ($1,$2,$3,$4,$5,$6,0,now())",
    )
    .bind(run_id)
    .bind(config.action.as_str())
    .bind(&config.actor)
    .bind(&config.reason)
    .bind(config.limit as i32)
    .bind(config.min_dead_age_seconds)
    .execute(pool)
    .await?;
    let after = load_summary(pool).await?;
    let summary = serde_json::json!({ "before": &before, "after": &after });
    sqlx::query("UPDATE content_index_recovery_runs SET summary = $2 WHERE id = $1")
        .bind(run_id)
        .bind(summary)
        .execute(pool)
        .await?;
    Ok((0, before, after))
}

async fn requeue_dead(
    pool: &sqlx::PgPool,
    run_id: Uuid,
    config: &Config,
) -> Result<(i64, Vec<OutboxStatus>, Vec<OutboxStatus>), RecoveryError> {
    let mut transaction = pool.begin().await?;
    let before = load_summary_in_transaction(&mut transaction).await?;
    sqlx::query(
        "INSERT INTO content_index_recovery_runs (id,action,actor,reason,requested_limit,min_dead_age_seconds) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(run_id)
    .bind(config.action.as_str())
    .bind(&config.actor)
    .bind(&config.reason)
    .bind(config.limit as i32)
    .bind(config.min_dead_age_seconds)
    .execute(&mut *transaction)
    .await?;

    let recovered_count = sqlx::query_scalar::<_, i64>(
        r#"
        WITH candidates AS (
            SELECT
                job.content_id,
                job.content_version AS previous_version,
                job.attempts AS previous_attempts,
                job.last_error
            FROM content_index_outbox AS job
            WHERE job.status = 'dead'
              AND job.updated_at <= now() - make_interval(secs => $1)
            ORDER BY job.updated_at, job.content_id
            FOR UPDATE SKIP LOCKED
            LIMIT $2
        ), requeued AS (
            UPDATE content_index_outbox AS job
            SET content_version = content.version,
                status = 'pending',
                attempts = 0,
                available_at = now(),
                locked_at = NULL,
                lease_id = NULL,
                last_error = NULL,
                updated_at = now()
            FROM candidates
            INNER JOIN content_items AS content ON content.id = candidates.content_id
            WHERE job.content_id = candidates.content_id
              AND job.status = 'dead'
            RETURNING
                job.content_id,
                candidates.previous_version,
                job.content_version AS requeued_version,
                candidates.previous_attempts,
                candidates.last_error
        ), audited AS (
            INSERT INTO content_index_recovery_items (
                run_id, content_id, previous_version, requeued_version,
                previous_attempts, previous_error
            )
            SELECT $3, content_id, previous_version, requeued_version,
                   previous_attempts, last_error
            FROM requeued
            RETURNING content_id
        )
        SELECT COUNT(*)::BIGINT FROM audited
        "#,
    )
    .bind(config.min_dead_age_seconds)
    .bind(config.limit)
    .bind(run_id)
    .fetch_one(&mut *transaction)
    .await?;
    let after = load_summary_in_transaction(&mut transaction).await?;
    let summary = serde_json::json!({ "before": &before, "after": &after });
    sqlx::query(
        "UPDATE content_index_recovery_runs SET recovered_count = $2, summary = $3, completed_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(recovered_count as i32)
    .bind(summary)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok((recovered_count, before, after))
}

async fn load_summary(pool: &sqlx::PgPool) -> Result<Vec<OutboxStatus>, sqlx::Error> {
    sqlx::query_as::<_, OutboxStatus>(SUMMARY_QUERY)
        .fetch_all(pool)
        .await
}

async fn load_summary_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<Vec<OutboxStatus>, sqlx::Error> {
    sqlx::query_as::<_, OutboxStatus>(SUMMARY_QUERY)
        .fetch_all(&mut **transaction)
        .await
}

const SUMMARY_QUERY: &str = r#"
    SELECT
        status,
        COUNT(*)::BIGINT AS count,
        COALESCE(MAX(attempts), 0)::INTEGER AS max_attempts,
        COALESCE(EXTRACT(EPOCH FROM now() - MIN(updated_at))::BIGINT, 0)
            AS oldest_age_seconds
    FROM content_index_outbox
    GROUP BY status
    ORDER BY status
"#;

#[cfg(test)]
mod tests {
    use super::{ConfigError, RecoveryAction, validate_approval};

    #[test]
    fn recovery_action_defaults_to_read_only_report_mode() {
        assert!(matches!(
            RecoveryAction::from_env_value("report").expect("valid report action"),
            RecoveryAction::Report
        ));
    }

    #[test]
    fn recovery_action_requires_explicit_dead_letter_requeue_mode() {
        assert!(matches!(
            RecoveryAction::from_env_value("requeue_dead").expect("valid requeue action"),
            RecoveryAction::RequeueDead
        ));
        assert!(matches!(
            RecoveryAction::from_env_value("requeue_all"),
            Err(ConfigError::Invalid { .. })
        ));
    }

    #[test]
    fn dead_letter_requeue_requires_a_named_actor_and_reason() {
        assert!(matches!(
            validate_approval(RecoveryAction::RequeueDead, None, Some("fixed")),
            Err(ConfigError::MissingApproval {
                key: "SEARCH_INDEX_RECOVERY_ACTOR"
            })
        ));
        assert!(matches!(
            validate_approval(RecoveryAction::RequeueDead, Some("oncall"), None),
            Err(ConfigError::MissingApproval {
                key: "SEARCH_INDEX_RECOVERY_REASON"
            })
        ));
        assert!(
            validate_approval(RecoveryAction::RequeueDead, Some("oncall"), Some("fixed")).is_ok()
        );
    }
}
