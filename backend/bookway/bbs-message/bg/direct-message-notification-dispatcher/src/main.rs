use std::{collections::HashMap, env, time::Duration};

use bookway_growth_api::pb::{self as growth_pb, growth_client::GrowthClient};
use futures::{StreamExt, stream};
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
        // Fail at startup when service authentication cannot be attached to
        // deliveries, instead of putting every claimed job into a retry loop.
        bookway_runtime::grpc_service_request(())?;
        Ok(Self {
            growth_url: env::var("GROWTH_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string()),
            batch_size: env_number("DIRECT_MESSAGE_NOTIFICATION_BATCH_SIZE", DEFAULT_BATCH_SIZE)?
                .clamp(1, 1_000),
            lease_seconds: env_number(
                "DIRECT_MESSAGE_NOTIFICATION_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?
            .clamp(5, 300),
            max_attempts: env_number(
                "DIRECT_MESSAGE_NOTIFICATION_MAX_ATTEMPTS",
                DEFAULT_MAX_ATTEMPTS,
            )?
            .clamp(1, 100),
            concurrency: env_number(
                "DIRECT_MESSAGE_NOTIFICATION_CONCURRENCY",
                DEFAULT_CONCURRENCY,
            )?
            .clamp(1, 128),
            request_timeout_ms: env_number(
                "DIRECT_MESSAGE_NOTIFICATION_REQUEST_TIMEOUT_MS",
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
    message_id: String,
    conversation_id: String,
    recipient_user_id: String,
    sender_user_id: String,
    lease_id: Uuid,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("direct-message-notification-dispatcher");
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
                                tracing::warn!(message_id = %job.message_id, %error, "direct message notification dispatch failed");
                            }
                        }
                    })
                    .await;
            }
            Err(error) => {
                tracing::error!(%error, "could not claim direct message notification jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn claim_jobs(pool: &sqlx::PgPool, config: &Config) -> Result<Vec<Job>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, (String, String, String, String, Uuid)>(
        r#"
        WITH candidates AS (
            SELECT message_id
            FROM direct_message_notification_jobs
            WHERE (status = 'pending' AND available_at <= now())
               OR (
                    status = 'processing'
                    AND COALESCE(locked_at, created_at) <= now() - make_interval(secs => $2)
               )
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE direct_message_notification_jobs AS job
        SET status = 'processing',
            attempts = job.attempts + 1,
            locked_at = now(),
            lease_id = gen_random_uuid(),
            updated_at = now()
        FROM candidates
        WHERE job.message_id = candidates.message_id
        RETURNING job.message_id, job.conversation_id, job.recipient_user_id,
                  job.sender_user_id, job.lease_id
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
            |(message_id, conversation_id, recipient_user_id, sender_user_id, lease_id)| Job {
                message_id,
                conversation_id,
                recipient_user_id,
                sender_user_id,
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
    let mut growth = growth.clone();
    let request = notification_request(job);
    match bookway_runtime::grpc_service_request(request) {
        Ok(request) => match tokio::time::timeout(
            Duration::from_millis(config.request_timeout_ms),
            growth.create_notification(request),
        )
        .await
        {
            Ok(Ok(_)) => mark_delivered(pool, job).await,
            Ok(Err(error)) => schedule_retry(pool, config, job, &error.to_string()).await,
            Err(_) => {
                schedule_retry(pool, config, job, "growth notification request timed out").await
            }
        },
        Err(error) => schedule_retry(pool, config, job, &error.to_string()).await,
    }
}

fn notification_request(job: &Job) -> growth_pb::CreateNotificationRequest {
    let mut data = HashMap::new();
    data.insert(
        "notification_type".to_string(),
        "direct_message".to_string(),
    );
    data.insert("message_id".to_string(), job.message_id.clone());
    data.insert("conversation_id".to_string(), job.conversation_id.clone());
    data.insert("sender_user_id".to_string(), job.sender_user_id.clone());
    growth_pb::CreateNotificationRequest {
        user_id: job.recipient_user_id.clone(),
        kind: growth_pb::NotificationKind::Community as i32,
        source_id: format!("direct-message:{}", job.message_id),
        title: "收到一条新私信".to_string(),
        // Never put private message text into a general notification inbox or
        // a downstream push-provider payload.
        body: "打开会话查看详情".to_string(),
        data,
    }
}

async fn mark_delivered(pool: &sqlx::PgPool, job: &Job) -> Result<(), String> {
    sqlx::query(
        "UPDATE direct_message_notification_jobs SET status='delivered',delivered_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE message_id=$1 AND lease_id=$2 AND status='processing'",
    )
    .bind(&job.message_id)
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
) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE direct_message_notification_jobs
        SET status = CASE WHEN attempts >= $3 THEN 'dead' ELSE 'pending' END,
            available_at = CASE
                WHEN attempts >= $3 THEN now()
                ELSE now() + make_interval(
                    secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))
                )
            END,
            locked_at = NULL,
            lease_id = NULL,
            last_error = left($4, 2000),
            updated_at = now()
        WHERE message_id=$1 AND lease_id=$2 AND status='processing'
        "#,
    )
    .bind(&job.message_id)
    .bind(job.lease_id)
    .bind(config.max_attempts)
    .bind(error)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{Job, notification_request};

    #[test]
    fn direct_message_notification_has_navigation_context_but_not_private_text() {
        let request = notification_request(&Job {
            message_id: "message-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            recipient_user_id: "recipient-1".to_string(),
            sender_user_id: "sender-1".to_string(),
            lease_id: Uuid::nil(),
        });

        assert_eq!(request.user_id, "recipient-1");
        assert_eq!(request.source_id, "direct-message:message-1");
        assert_eq!(
            request.data.get("conversation_id"),
            Some(&"conversation-1".to_string())
        );
        assert_eq!(
            request.data.get("sender_user_id"),
            Some(&"sender-1".to_string())
        );
        assert_eq!(request.body, "打开会话查看详情");
    }
}
