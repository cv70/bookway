use std::{collections::HashMap, env, time::Duration};

use bookway_growth_api::pb::{self as growth_pb, growth_client::GrowthClient};
use futures::{StreamExt, stream};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const DEFAULT_BATCH_SIZE: i64 = 100;
const DEFAULT_LEASE_SECONDS: i32 = 30;
const DEFAULT_MAX_ATTEMPTS: i32 = 10;
const DEFAULT_CONCURRENCY: usize = 16;
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 3_000;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
    #[error("invalid service authentication configuration: {0}")]
    ServiceAuth(#[from] bookway_runtime::GrpcServiceAuthError),
}

struct Config {
    growth_url: String,
    batch_size: i64,
    lease_seconds: i32,
    max_attempts: i32,
    concurrency: usize,
    request_timeout_ms: u64,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        // Fail during startup rather than turning every job into a retry when
        // service authentication is enabled but not configured.
        bookway_runtime::grpc_service_request(())?;
        Ok(Self {
            growth_url: env::var("GROWTH_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string()),
            batch_size: env_number("COMMUNITY_NOTIFICATION_BATCH_SIZE", DEFAULT_BATCH_SIZE)?
                .clamp(1, 1_000),
            lease_seconds: env_number(
                "COMMUNITY_NOTIFICATION_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?
            .clamp(5, 300),
            max_attempts: env_number("COMMUNITY_NOTIFICATION_MAX_ATTEMPTS", DEFAULT_MAX_ATTEMPTS)?
                .clamp(1, 100),
            concurrency: env_number("COMMUNITY_NOTIFICATION_CONCURRENCY", DEFAULT_CONCURRENCY)?
                .clamp(1, 128),
            request_timeout_ms: env_number(
                "COMMUNITY_NOTIFICATION_REQUEST_TIMEOUT_MS",
                DEFAULT_REQUEST_TIMEOUT_MS,
            )?
            .clamp(100, 30_000),
        })
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

struct Job {
    source_id: String,
    recipient_user_id: String,
    title: String,
    body: String,
    data: Value,
    lease_id: Uuid,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("community-notification-dispatcher");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let growth = GrowthClient::connect(config.growth_url.clone()).await?;

    loop {
        match claim_jobs(&pool, &config).await {
            Ok(jobs) if jobs.is_empty() => tokio::time::sleep(Duration::from_millis(500)).await,
            Ok(jobs) => {
                let concurrency = config.concurrency;
                let dispatch_config = &config;
                stream::iter(jobs)
                    .for_each_concurrent(concurrency, |job| {
                        let pool = pool.clone();
                        let growth = growth.clone();
                        async move {
                            if let Err(error) = dispatch(&pool, &growth, dispatch_config, &job).await
                            {
                                tracing::warn!(source_id = %job.source_id, %error, "community notification dispatch failed");
                            }
                        }
                    })
                    .await;
            }
            Err(error) => {
                tracing::error!(%error, "could not claim community notification jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn claim_jobs(pool: &sqlx::PgPool, config: &Config) -> Result<Vec<Job>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, (String, String, String, String, Value, Uuid)>(
        r#"
        WITH candidates AS (
            SELECT source_id
            FROM community_notification_jobs
            WHERE (status = 'pending' AND available_at <= now())
               OR (
                    status = 'processing'
                    AND COALESCE(locked_at, created_at) <= now() - make_interval(secs => $2)
               )
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE community_notification_jobs AS job
        SET status = 'processing',
            attempts = job.attempts + 1,
            locked_at = now(),
            lease_id = gen_random_uuid(),
            updated_at = now()
        FROM candidates
        WHERE job.source_id = candidates.source_id
        RETURNING job.source_id, job.recipient_user_id, job.title, job.body, job.data, job.lease_id
        "#,
    )
    .bind(config.batch_size)
    .bind(config.lease_seconds)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(source_id, recipient_user_id, title, body, data, lease_id)| Job {
                source_id,
                recipient_user_id,
                title,
                body,
                data,
                lease_id,
            },
        )
        .collect())
}

async fn dispatch(
    pool: &sqlx::PgPool,
    growth: &GrowthClient<tonic::transport::Channel>,
    config: &Config,
    job: &Job,
) -> Result<(), String> {
    let request = match notification_request(job) {
        Ok(request) => request,
        Err(error) => {
            return schedule_retry(pool, config, job, &error.to_string(), true).await;
        }
    };
    let mut growth = growth.clone();
    match bookway_runtime::grpc_service_request(request) {
        Ok(request) => match tokio::time::timeout(
            Duration::from_millis(config.request_timeout_ms),
            growth.create_notification(request),
        )
        .await
        {
            Ok(Ok(_)) => mark_delivered(pool, job).await,
            Ok(Err(error)) => schedule_retry(pool, config, job, &error.to_string(), false).await,
            Err(_) => {
                schedule_retry(
                    pool,
                    config,
                    job,
                    "growth notification request timed out",
                    false,
                )
                .await
            }
        },
        Err(error) => schedule_retry(pool, config, job, &error.to_string(), false).await,
    }
}

fn notification_request(
    job: &Job,
) -> Result<growth_pb::CreateNotificationRequest, serde_json::Error> {
    Ok(growth_pb::CreateNotificationRequest {
        user_id: job.recipient_user_id.clone(),
        kind: growth_pb::NotificationKind::Community as i32,
        source_id: job.source_id.clone(),
        title: job.title.clone(),
        body: job.body.clone(),
        data: serde_json::from_value::<HashMap<String, String>>(job.data.clone())?,
    })
}

async fn mark_delivered(pool: &sqlx::PgPool, job: &Job) -> Result<(), String> {
    sqlx::query(
        "UPDATE community_notification_jobs SET status='delivered',delivered_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE source_id=$1 AND lease_id=$2 AND status='processing'",
    )
    .bind(&job.source_id)
    .bind(job.lease_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn schedule_retry(
    pool: &sqlx::PgPool,
    config: &Config,
    job: &Job,
    error: &str,
    terminal: bool,
) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE community_notification_jobs
        SET status = CASE WHEN $3 OR attempts >= $4 THEN 'dead' ELSE 'pending' END,
            available_at = CASE
                WHEN $3 OR attempts >= $4 THEN now()
                ELSE now() + make_interval(
                    secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))
                )
            END,
            locked_at = NULL,
            lease_id = NULL,
            last_error = left($5, 2000),
            updated_at = now()
        WHERE source_id=$1 AND lease_id=$2 AND status='processing'
        "#,
    )
    .bind(&job.source_id)
    .bind(job.lease_id)
    .bind(terminal)
    .bind(config.max_attempts)
    .bind(error)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::{Job, notification_request};

    #[test]
    fn notification_request_preserves_the_durable_payload() {
        let request = notification_request(&Job {
            source_id: "like:reader:post-1".to_string(),
            recipient_user_id: "author".to_string(),
            title: "收到一个赞".to_string(),
            body: "有人赞了你的内容".to_string(),
            data: json!({"actor_id":"reader", "post_id":"post-1"}),
            lease_id: Uuid::nil(),
        })
        .expect("stored gateway data is a string map");

        assert_eq!(request.user_id, "author");
        assert_eq!(request.source_id, "like:reader:post-1");
        assert_eq!(request.data.get("actor_id"), Some(&"reader".to_string()));
    }
}
