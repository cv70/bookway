use std::{env, sync::Arc, time::Duration};

use async_trait::async_trait;
use bookway_api::{
    ContentDto, ContentStatusDto, CreateUserNotificationRequest, NotificationKindDto,
};
use futures::{StreamExt, stream};
use sqlx::PgPool;
use thiserror::Error;
use tonic::{
    Request,
    metadata::{Ascii, MetadataValue},
};

const MAX_DELIVERY_ATTEMPTS: i32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecisionStatus {
    Resolved,
    Rejected,
}

impl DecisionStatus {
    fn parse(value: String) -> Result<Self, sqlx::Error> {
        match value.as_str() {
            "resolved" => Ok(Self::Resolved),
            "rejected" => Ok(Self::Rejected),
            _ => Err(sqlx::Error::Protocol(format!(
                "invalid appeal decision status: {value}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentAction {
    NoAction,
    RestoreContent,
}

impl ContentAction {
    fn parse(value: String) -> Result<Self, sqlx::Error> {
        match value.as_str() {
            "no_action" => Ok(Self::NoAction),
            "restore_content" => Ok(Self::RestoreContent),
            _ => Err(sqlx::Error::Protocol(format!(
                "invalid appeal notification action: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppealNotificationJob {
    appeal_id: String,
    user_id: String,
    content_id: String,
    decision_status: DecisionStatus,
    action: ContentAction,
    resolution: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeliveryDisposition {
    Delivered,
    Deferred,
}

#[derive(Debug, Error)]
enum DispatcherError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
enum TargetError {
    #[error("response serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("bbs-link request failed: {0}")]
    BbsLink(tonic::Status),
    #[error("growth request failed: {0}")]
    Growth(tonic::Status),
    #[error("delivery request timed out after {0}ms")]
    Timeout(u64),
    #[error("restore decision found content {0} outside the public state")]
    ContentNotPublic(String),
}

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
    #[error("SERVICE_AUTH_TOKEN is required when SERVICE_AUTH_REQUIRED=true")]
    MissingServiceAuthToken,
    #[error("SERVICE_AUTH_TOKEN is invalid gRPC metadata")]
    InvalidServiceAuthToken,
}

#[async_trait]
trait JobRepository: Send + Sync {
    async fn claim(&self) -> Result<Vec<AppealNotificationJob>, sqlx::Error>;
    async fn mark_delivered(&self, job: &AppealNotificationJob) -> Result<(), sqlx::Error>;
    async fn mark_failed(
        &self,
        job: &AppealNotificationJob,
        error: &str,
    ) -> Result<(), sqlx::Error>;
}

#[async_trait]
trait NotificationTarget: Send + Sync {
    async fn deliver(
        &self,
        job: &AppealNotificationJob,
    ) -> Result<DeliveryDisposition, TargetError>;
}

struct PostgresJobRepository {
    pool: PgPool,
    batch_size: i64,
    lease_seconds: i32,
}

impl PostgresJobRepository {
    fn new(pool: PgPool, batch_size: i64, lease_seconds: i32) -> Self {
        Self {
            pool,
            batch_size,
            lease_seconds,
        }
    }
}

#[async_trait]
impl JobRepository for PostgresJobRepository {
    async fn claim(&self) -> Result<Vec<AppealNotificationJob>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String)>(
            "WITH claimed AS (SELECT appeal_id FROM content_appeal_notification_jobs WHERE ((delivery_status = 'pending' AND available_at <= now()) OR (delivery_status = 'dispatching' AND lease_until <= now())) ORDER BY available_at,created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE content_appeal_notification_jobs j SET delivery_status = 'dispatching',attempts = attempts + 1,lease_until = now() + make_interval(secs => $2),updated_at = now() FROM claimed WHERE j.appeal_id = claimed.appeal_id RETURNING j.appeal_id,j.user_id,j.content_id,j.decision_status,j.action,j.resolution",
        )
        .bind(self.batch_size)
        .bind(self.lease_seconds)
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        rows.into_iter()
            .map(
                |(appeal_id, user_id, content_id, decision_status, action, resolution)| {
                    Ok(AppealNotificationJob {
                        appeal_id,
                        user_id,
                        content_id,
                        decision_status: DecisionStatus::parse(decision_status)?,
                        action: ContentAction::parse(action)?,
                        resolution,
                    })
                },
            )
            .collect()
    }

    async fn mark_delivered(&self, job: &AppealNotificationJob) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE content_appeal_notification_jobs SET delivery_status = 'delivered',lease_until = NULL,delivered_at = now(),last_error = NULL,updated_at = now() WHERE appeal_id = $1 AND delivery_status = 'dispatching'",
        )
        .bind(&job.appeal_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_failed(
        &self,
        job: &AppealNotificationJob,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE content_appeal_notification_jobs SET delivery_status = CASE WHEN attempts >= $2 THEN 'dead' ELSE 'pending' END,lease_until = NULL,available_at = now() + make_interval(secs => LEAST(300, CAST(power(2, LEAST(attempts, 8)) AS INTEGER)) + floor(random() * 3)::INTEGER),last_error = left($3,2000),updated_at = now() WHERE appeal_id = $1 AND delivery_status = 'dispatching'",
        )
        .bind(&job.appeal_id)
        .bind(MAX_DELIVERY_ATTEMPTS)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

struct GrpcNotificationTarget {
    bbs_link: bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient<tonic::transport::Channel>,
    growth: bookway_growth::api::pb::growth_client::GrowthClient<tonic::transport::Channel>,
    service_auth_token: Option<MetadataValue<Ascii>>,
}

impl GrpcNotificationTarget {
    async fn connect(
        bbs_link_url: String,
        growth_url: String,
        service_auth_token: Option<MetadataValue<Ascii>>,
    ) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            bbs_link: bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient::connect(
                bbs_link_url,
            )
            .await?,
            growth: bookway_growth::api::pb::growth_client::GrowthClient::connect(growth_url)
                .await?,
            service_auth_token,
        })
    }
}

fn bbs_link_request<T>(
    message: T,
    service_auth_token: Option<&MetadataValue<Ascii>>,
) -> Request<T> {
    let mut request = Request::new(message);
    if let Some(token) = service_auth_token {
        request
            .metadata_mut()
            .insert("x-service-token", token.clone());
    }
    request
}

#[async_trait]
impl NotificationTarget for GrpcNotificationTarget {
    async fn deliver(
        &self,
        job: &AppealNotificationJob,
    ) -> Result<DeliveryDisposition, TargetError> {
        if job.action == ContentAction::RestoreContent {
            let mut bbs_link = self.bbs_link.clone();
            bbs_link
                .restore(bbs_link_request(
                    bookway_bbs_link::api::pb::RestoreRequest {
                        content_id: job.content_id.clone(),
                    },
                    self.service_auth_token.as_ref(),
                ))
                .await
                .map_err(TargetError::BbsLink)?;
            let response = match bbs_link
                .get_public(bbs_link_request(
                    bookway_bbs_link::api::pb::IdRequest {
                        id: job.content_id.clone(),
                    },
                    self.service_auth_token.as_ref(),
                ))
                .await
            {
                Ok(response) => response.into_inner(),
                Err(status) if status.code() == tonic::Code::NotFound => {
                    // The restore write can reach a read replica before the public
                    // read catches up. Do not expose the decision until readback.
                    return Ok(DeliveryDisposition::Deferred);
                }
                Err(status) => return Err(TargetError::BbsLink(status)),
            };
            let content: ContentDto = serde_json::from_str(&response.response_json)?;
            if content.status != ContentStatusDto::Published {
                return Err(TargetError::ContentNotPublic(job.content_id.clone()));
            }
        }

        let request = notification_request(job);
        let mut growth = self.growth.clone();
        growth
            .create_notification(bookway_growth::api::pb::CreateNotificationRequest {
                user_id: job.user_id.clone(),
                request_json: serde_json::to_string(&request)?,
            })
            .await
            .map_err(TargetError::Growth)?;
        Ok(DeliveryDisposition::Delivered)
    }
}

fn notification_request(job: &AppealNotificationJob) -> CreateUserNotificationRequest {
    let restored = job.action == ContentAction::RestoreContent;
    CreateUserNotificationRequest {
        kind: NotificationKindDto::Community,
        source_id: format!(
            "content-appeal:{}:{}",
            job.appeal_id,
            job.decision_status.as_str()
        ),
        title: if restored {
            "你的内容申诉已通过".to_string()
        } else {
            "你的内容申诉已有结果".to_string()
        },
        body: if job.resolution.is_empty() {
            if restored {
                "复核已通过，内容现已恢复公开。".to_string()
            } else {
                "请在创作中心查看审核说明。".to_string()
            }
        } else {
            job.resolution.clone()
        },
        data: serde_json::json!({
            "appeal_id": job.appeal_id,
            "post_id": job.content_id,
            "appeal_status": job.decision_status.as_str(),
            "content_restored": restored,
        }),
    }
}

struct Dispatcher<R, T> {
    repository: Arc<R>,
    target: Arc<T>,
    concurrency: usize,
    request_timeout: Duration,
}

impl<R, T> Dispatcher<R, T>
where
    R: JobRepository + 'static,
    T: NotificationTarget + 'static,
{
    fn new(
        repository: Arc<R>,
        target: Arc<T>,
        concurrency: usize,
        request_timeout: Duration,
    ) -> Self {
        Self {
            repository,
            target,
            concurrency,
            request_timeout,
        }
    }

    async fn run_once(&self) -> Result<usize, DispatcherError> {
        let jobs = self.repository.claim().await?;
        let count = jobs.len();
        let results = stream::iter(jobs)
            .map(|job| self.dispatch(job))
            .buffer_unordered(self.concurrency)
            .collect::<Vec<_>>()
            .await;
        for result in results {
            result?;
        }
        Ok(count)
    }

    async fn dispatch(&self, job: AppealNotificationJob) -> Result<(), DispatcherError> {
        let delivery = tokio::time::timeout(self.request_timeout, self.target.deliver(&job)).await;
        match delivery {
            Ok(Ok(DeliveryDisposition::Delivered)) => {
                self.repository.mark_delivered(&job).await?;
                tracing::debug!(appeal_id = %job.appeal_id, "appeal notification delivered");
            }
            Ok(Ok(DeliveryDisposition::Deferred)) => {
                self.repository
                    .mark_failed(&job, "waiting for restored content to become public")
                    .await?;
                tracing::debug!(appeal_id = %job.appeal_id, "appeal notification deferred");
            }
            Ok(Err(error)) => {
                self.repository
                    .mark_failed(&job, &error.to_string())
                    .await?;
                tracing::warn!(appeal_id = %job.appeal_id, error = %error, "appeal notification delivery failed");
            }
            Err(_) => {
                let error = TargetError::Timeout(self.request_timeout.as_millis() as u64);
                self.repository
                    .mark_failed(&job, &error.to_string())
                    .await?;
                tracing::warn!(appeal_id = %job.appeal_id, error = %error, "appeal notification delivery timed out");
            }
        }
        Ok(())
    }
}

struct Config {
    bbs_link_url: String,
    growth_url: String,
    service_auth_token: Option<MetadataValue<Ascii>>,
    batch_size: i64,
    concurrency: usize,
    lease_seconds: i32,
    request_timeout: Duration,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            growth_url: env::var("GROWTH_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string()),
            service_auth_token: service_auth_token()?,
            batch_size: env_number("APPEAL_NOTIFICATION_BATCH_SIZE", 100_i64)?.clamp(1, 1_000),
            concurrency: env_number("APPEAL_NOTIFICATION_CONCURRENCY", 16_usize)?.clamp(1, 128),
            lease_seconds: env_number("APPEAL_NOTIFICATION_LEASE_SECONDS", 30_i32)?.clamp(5, 300),
            request_timeout: Duration::from_millis(
                env_number("APPEAL_NOTIFICATION_REQUEST_TIMEOUT_MS", 3_000_u64)?.clamp(100, 30_000),
            ),
        })
    }
}

fn service_auth_token() -> Result<Option<MetadataValue<Ascii>>, ConfigError> {
    if !env::var("SERVICE_AUTH_REQUIRED").is_ok_and(|value| value == "true") {
        return Ok(None);
    }
    let token = env::var("SERVICE_AUTH_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return Err(ConfigError::MissingServiceAuthToken);
    }
    let value = MetadataValue::try_from(token.as_str())
        .map_err(|_| ConfigError::InvalidServiceAuthToken)?;
    Ok(Some(value))
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
    bookway_runtime::init_tracing("appeal-notification-dispatcher");
    let config = Config::from_env()?;
    let repository = Arc::new(PostgresJobRepository::new(
        bookway_data::postgres_pool().await?,
        config.batch_size,
        config.lease_seconds,
    ));
    let target = Arc::new(
        GrpcNotificationTarget::connect(
            config.bbs_link_url,
            config.growth_url,
            config.service_auth_token,
        )
        .await?,
    );
    let dispatcher = Dispatcher::new(
        repository,
        target,
        config.concurrency,
        config.request_timeout,
    );
    loop {
        match dispatcher.run_once().await {
            Ok(0) => tokio::time::sleep(Duration::from_millis(500)).await,
            Ok(count) => tracing::debug!(count, "appeal notification jobs dispatched"),
            Err(error) => {
                tracing::error!(%error, "appeal notification dispatcher iteration failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MemoryRepository {
        pending: Mutex<Vec<AppealNotificationJob>>,
        delivered: Mutex<Vec<String>>,
        failed: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl JobRepository for MemoryRepository {
        async fn claim(&self) -> Result<Vec<AppealNotificationJob>, sqlx::Error> {
            Ok(std::mem::take(
                &mut *self.pending.lock().expect("pending lock"),
            ))
        }

        async fn mark_delivered(&self, job: &AppealNotificationJob) -> Result<(), sqlx::Error> {
            self.delivered
                .lock()
                .expect("delivered lock")
                .push(job.appeal_id.clone());
            Ok(())
        }

        async fn mark_failed(
            &self,
            job: &AppealNotificationJob,
            error: &str,
        ) -> Result<(), sqlx::Error> {
            self.failed
                .lock()
                .expect("failed lock")
                .push((job.appeal_id.clone(), error.to_string()));
            Ok(())
        }
    }

    struct RecordingTarget {
        disposition: Result<DeliveryDisposition, TargetError>,
        received: Mutex<Vec<AppealNotificationJob>>,
    }

    #[async_trait]
    impl NotificationTarget for RecordingTarget {
        async fn deliver(
            &self,
            job: &AppealNotificationJob,
        ) -> Result<DeliveryDisposition, TargetError> {
            self.received
                .lock()
                .expect("received lock")
                .push(job.clone());
            match &self.disposition {
                Ok(disposition) => Ok(*disposition),
                Err(error) => Err(TargetError::ContentNotPublic(error.to_string())),
            }
        }
    }

    fn job(action: ContentAction) -> AppealNotificationJob {
        AppealNotificationJob {
            appeal_id: "appeal-1".to_string(),
            user_id: "author-1".to_string(),
            content_id: "content-1".to_string(),
            decision_status: DecisionStatus::Resolved,
            action,
            resolution: "复核结论".to_string(),
        }
    }

    #[tokio::test]
    async fn delivered_jobs_are_marked_once() {
        let repository = Arc::new(MemoryRepository {
            pending: Mutex::new(vec![job(ContentAction::NoAction)]),
            ..Default::default()
        });
        let target = Arc::new(RecordingTarget {
            disposition: Ok(DeliveryDisposition::Delivered),
            received: Mutex::new(Vec::new()),
        });
        let dispatcher = Dispatcher::new(
            Arc::clone(&repository),
            Arc::clone(&target),
            1,
            Duration::from_secs(1),
        );

        assert_eq!(dispatcher.run_once().await.expect("dispatch"), 1);
        assert_eq!(
            repository
                .delivered
                .lock()
                .expect("delivered lock")
                .as_slice(),
            ["appeal-1"]
        );
        assert!(repository.failed.lock().expect("failed lock").is_empty());
        assert_eq!(target.received.lock().expect("received lock").len(), 1);
    }

    #[tokio::test]
    async fn deferred_restores_remain_retryable() {
        let repository = Arc::new(MemoryRepository {
            pending: Mutex::new(vec![job(ContentAction::RestoreContent)]),
            ..Default::default()
        });
        let target = Arc::new(RecordingTarget {
            disposition: Ok(DeliveryDisposition::Deferred),
            received: Mutex::new(Vec::new()),
        });
        let dispatcher =
            Dispatcher::new(Arc::clone(&repository), target, 1, Duration::from_secs(1));

        assert_eq!(dispatcher.run_once().await.expect("dispatch"), 1);
        assert!(
            repository
                .delivered
                .lock()
                .expect("delivered lock")
                .is_empty()
        );
        let failures = repository.failed.lock().expect("failed lock");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, "appeal-1");
    }

    #[test]
    fn restored_notification_has_a_stable_idempotency_key_and_navigation_data() {
        let request = notification_request(&job(ContentAction::RestoreContent));

        assert_eq!(request.source_id, "content-appeal:appeal-1:resolved");
        assert_eq!(request.kind, NotificationKindDto::Community);
        assert_eq!(request.data["appeal_id"], "appeal-1");
        assert_eq!(request.data["content_restored"], true);
    }

    #[test]
    fn restore_request_carries_the_worker_service_token() {
        let token: MetadataValue<Ascii> = "dispatcher-token".try_into().expect("valid token");
        let request = bbs_link_request(
            bookway_bbs_link::api::pb::RestoreRequest {
                content_id: "content-1".to_string(),
            },
            Some(&token),
        );

        assert_eq!(request.get_ref().content_id, "content-1");
        assert_eq!(
            request
                .metadata()
                .get("x-service-token")
                .and_then(|value| value.to_str().ok()),
            Some("dispatcher-token")
        );
    }
}
