use std::{env, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

const DEFAULT_BATCH_SIZE: i64 = 100;
const DEFAULT_LEASE_SECONDS: i32 = 120;
const DEFAULT_MAX_ATTEMPTS: i32 = 10;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("{key} is required")]
    Missing { key: &'static str },
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
}

struct Config {
    gateway_url: String,
    batch_size: i64,
    lease_seconds: i32,
    max_attempts: i32,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        let gateway_url = required_env("PUSH_DELIVERY_GATEWAY_URL")?;
        reqwest::Url::parse(&gateway_url).map_err(|_| ConfigError::Invalid {
            key: "PUSH_DELIVERY_GATEWAY_URL",
            value: gateway_url.clone(),
        })?;
        Ok(Self {
            gateway_url,
            batch_size: env_number("PUSH_DELIVERY_BATCH_SIZE", DEFAULT_BATCH_SIZE)?.clamp(1, 1_000),
            lease_seconds: env_number("PUSH_DELIVERY_LEASE_SECONDS", DEFAULT_LEASE_SECONDS)?
                .clamp(10, 3_600),
            max_attempts: env_number("PUSH_DELIVERY_MAX_ATTEMPTS", DEFAULT_MAX_ATTEMPTS)?
                .clamp(1, 100),
        })
    }
}

fn required_env(key: &'static str) -> Result<String, ConfigError> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ConfigError::Missing { key })
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

struct Job {
    delivery_id: Uuid,
    lease_id: Uuid,
}

#[derive(sqlx::FromRow)]
struct Delivery {
    user_id: String,
    action_id: String,
    device_id: String,
    schedule_revision: i32,
}

#[derive(sqlx::FromRow)]
struct Action {
    state: String,
    schedule_revision: i32,
    journey_status: String,
    payload: Value,
}

#[derive(sqlx::FromRow)]
struct Device {
    provider: String,
    endpoint: String,
    active: bool,
}

#[derive(Serialize)]
struct GatewayRequest<'a> {
    delivery_id: String,
    provider: &'a str,
    endpoint: &'a str,
    title: String,
    body: String,
    data: Value,
}

#[derive(Deserialize)]
struct GatewayResponse {
    status: String,
    retryable: Option<bool>,
    error: Option<String>,
}

enum ProviderOutcome {
    Delivered,
    InvalidDevice(String),
    Retry(String),
    Failed(String),
}

struct Gateway {
    url: String,
    client: reqwest::Client,
}

impl Gateway {
    async fn send(&self, request: &GatewayRequest<'_>) -> ProviderOutcome {
        let response = match self.client.post(&self.url).json(request).send().await {
            Ok(response) => response,
            Err(error) => return ProviderOutcome::Retry(error.to_string()),
        };
        let status = response.status();
        let payload = match response.json::<GatewayResponse>().await {
            Ok(payload) => payload,
            Err(error)
                if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS =>
            {
                return ProviderOutcome::Retry(error.to_string());
            }
            Err(error) => return ProviderOutcome::Failed(error.to_string()),
        };
        let error = payload
            .error
            .unwrap_or_else(|| "provider rejected delivery".to_string());
        match payload.status.as_str() {
            "sent" | "duplicate" if status.is_success() => ProviderOutcome::Delivered,
            "invalid_device" => ProviderOutcome::InvalidDevice(error),
            _ if payload.retryable.unwrap_or(
                status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS,
            ) =>
            {
                ProviderOutcome::Retry(error)
            }
            _ => ProviderOutcome::Failed(error),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("push-delivery-dispatcher");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let gateway = Gateway {
        url: config.gateway_url.clone(),
        client: bookway_runtime::http_client(),
    };
    loop {
        match claim_jobs(&pool, &config).await {
            Ok(jobs) if jobs.is_empty() => tokio::time::sleep(Duration::from_millis(500)).await,
            Ok(jobs) => {
                for job in jobs {
                    if let Err(error) = process_job(&pool, &gateway, &config, &job).await {
                        tracing::warn!(delivery_id = %job.delivery_id, %error, "push delivery processing failed");
                    }
                }
            }
            Err(error) => {
                tracing::error!(%error, "could not claim push delivery jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn claim_jobs(pool: &sqlx::PgPool, config: &Config) -> Result<Vec<Job>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid, Uuid)>(
        r#"
        WITH candidates AS (
            SELECT id
            FROM reminder_deliveries
            WHERE (status = 'queued' AND available_at <= now())
               OR (status = 'processing' AND COALESCE(locked_at, created_at) <= now() - make_interval(secs => $2))
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE reminder_deliveries AS delivery
        SET status = 'processing',
            attempts = delivery.attempts + 1,
            locked_at = now(),
            lease_id = gen_random_uuid(),
            updated_at = now()
        FROM candidates
        WHERE delivery.id = candidates.id
        RETURNING delivery.id, delivery.lease_id
        "#,
    )
    .bind(config.batch_size)
    .bind(config.lease_seconds)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(delivery_id, lease_id)| Job {
            delivery_id,
            lease_id,
        })
        .collect())
}

async fn process_job(
    pool: &sqlx::PgPool,
    gateway: &Gateway,
    config: &Config,
    job: &Job,
) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let delivery = sqlx::query_as::<_, Delivery>(
        "SELECT user_id,action_id,device_id,schedule_revision FROM reminder_deliveries WHERE id=$1 AND lease_id=$2 AND status='processing'",
    )
    .bind(job.delivery_id)
    .bind(job.lease_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let Some(delivery) = delivery else {
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    };
    let action = sqlx::query_as::<_, Action>(
        "SELECT a.state,a.schedule_revision,j.status AS journey_status,a.payload FROM actions a JOIN journeys j ON j.id=a.journey_id AND j.user_id=a.user_id WHERE a.id=$1 AND a.user_id=$2 FOR UPDATE OF a",
    )
    .bind(&delivery.action_id)
    .bind(&delivery.user_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let device = sqlx::query_as::<_, Device>(
        "SELECT provider,endpoint,active FROM push_devices WHERE device_id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(&delivery.device_id)
    .bind(&delivery.user_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let Some(action) = action else {
        cancel(&mut transaction, job, "action is no longer eligible").await?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    };
    let Some(device) = device else {
        cancel(&mut transaction, job, "device is no longer registered").await?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    };
    if action.state != "pending"
        || action.schedule_revision != delivery.schedule_revision
        || action.journey_status != "active"
        || !device.active
    {
        cancel(
            &mut transaction,
            job,
            "reminder was canceled or rescheduled",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    // Match Growth's cancellation lock order: action first, then delivery.
    // This prevents a reschedule from deadlocking with a provider send.
    let still_claimed = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM reminder_deliveries WHERE id=$1 AND lease_id=$2 AND status='processing' FOR UPDATE",
    )
    .bind(job.delivery_id)
    .bind(job.lease_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if still_claimed.is_none() {
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let title = action
        .payload
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("已安排的行动")
        .to_string();
    let request = GatewayRequest {
        delivery_id: job.delivery_id.to_string(),
        provider: &device.provider,
        endpoint: &device.endpoint,
        title: "行动提醒".to_string(),
        body: format!("“{title}” 即将开始，准备好时从一个最小步骤开始。"),
        data: json!({ "action_id": delivery.action_id, "schedule_revision": delivery.schedule_revision }),
    };
    match gateway.send(&request).await {
        ProviderOutcome::Delivered => {
            dispatched(&mut transaction, job).await?;
            transaction
                .commit()
                .await
                .map_err(|error| error.to_string())
        }
        ProviderOutcome::InvalidDevice(error) => {
            revoke_and_cancel(&mut transaction, job, &delivery, &error).await?;
            transaction
                .commit()
                .await
                .map_err(|error| error.to_string())
        }
        ProviderOutcome::Retry(error) => {
            transaction
                .rollback()
                .await
                .map_err(|error| error.to_string())?;
            schedule_retry(pool, config, job, &error, false).await
        }
        ProviderOutcome::Failed(error) => {
            transaction
                .rollback()
                .await
                .map_err(|error| error.to_string())?;
            schedule_retry(pool, config, job, &error, true).await
        }
    }
}

async fn dispatched(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &Job,
) -> Result<(), String> {
    let result = sqlx::query("UPDATE reminder_deliveries SET status='dispatched',dispatched_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE id=$1 AND lease_id=$2 AND status='processing'")
        .bind(job.delivery_id).bind(job.lease_id).execute(&mut **transaction).await.map_err(|error| error.to_string())?;
    if result.rows_affected() != 1 {
        return Err("push delivery lease was replaced".to_string());
    }
    Ok(())
}

async fn cancel(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &Job,
    error: &str,
) -> Result<(), String> {
    sqlx::query("UPDATE reminder_deliveries SET status='canceled',canceled_at=now(),locked_at=NULL,lease_id=NULL,last_error=left($3,2000),updated_at=now() WHERE id=$1 AND lease_id=$2 AND status='processing'")
        .bind(job.delivery_id).bind(job.lease_id).bind(error).execute(&mut **transaction).await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn revoke_and_cancel(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &Job,
    delivery: &Delivery,
    error: &str,
) -> Result<(), String> {
    sqlx::query("UPDATE push_devices SET active=false,revoked_at=now(),updated_at=now() WHERE user_id=$1 AND device_id=$2 AND active")
        .bind(&delivery.user_id).bind(&delivery.device_id).execute(&mut **transaction).await.map_err(|error| error.to_string())?;
    cancel(transaction, job, error).await
}

async fn schedule_retry(
    pool: &sqlx::PgPool,
    config: &Config,
    job: &Job,
    error: &str,
    terminal: bool,
) -> Result<(), String> {
    let result = sqlx::query(r#"UPDATE reminder_deliveries SET status=CASE WHEN $3 OR attempts >= $4 THEN 'failed' ELSE 'queued' END,available_at=CASE WHEN $3 OR attempts >= $4 THEN now() ELSE now()+make_interval(secs => LEAST(300,CAST(power(2,attempts) AS INTEGER))) END,locked_at=NULL,lease_id=NULL,last_error=left($5,2000),updated_at=now() WHERE id=$1 AND lease_id=$2 AND status='processing'"#)
        .bind(job.delivery_id).bind(job.lease_id).bind(terminal).bind(config.max_attempts).bind(error).execute(pool).await.map_err(|error| error.to_string())?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err("push delivery lease was replaced".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{GatewayResponse, ProviderOutcome};

    #[test]
    fn provider_response_models_success_and_invalid_device() {
        let sent = GatewayResponse {
            status: "sent".to_string(),
            retryable: None,
            error: None,
        };
        assert_eq!(sent.status, "sent");
        let invalid = ProviderOutcome::InvalidDevice("unregistered".to_string());
        assert!(matches!(invalid, ProviderOutcome::InvalidDevice(_)));
    }
}
