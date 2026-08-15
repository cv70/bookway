use std::{env, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::{StreamExt, stream};
use sqlx::PgPool;
use thiserror::Error;
use tonic::{
    Request,
    metadata::{Ascii, MetadataValue},
};

const MAX_DELIVERY_ATTEMPTS: i32 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RestrictionJob {
    report_id: String,
    content_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeliveryDisposition {
    Restricted,
    Deferred,
}

#[derive(Debug, Error)]
enum DispatcherError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
enum TargetError {
    #[error("bbs-link request failed: {0}")]
    BbsLink(tonic::Status),
    #[error("restriction request timed out after {0}ms")]
    Timeout(u64),
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
    async fn claim(&self) -> Result<Vec<RestrictionJob>, sqlx::Error>;
    async fn mark_delivered(&self, job: &RestrictionJob) -> Result<(), sqlx::Error>;
    async fn mark_failed(&self, job: &RestrictionJob, error: &str) -> Result<(), sqlx::Error>;
}

#[async_trait]
trait RestrictionTarget: Send + Sync {
    async fn restrict(&self, job: &RestrictionJob) -> Result<DeliveryDisposition, TargetError>;
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
    async fn claim(&self) -> Result<Vec<RestrictionJob>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (String, String)>(
            "WITH claimed AS (SELECT report_id FROM content_report_restriction_jobs WHERE ((delivery_status = 'pending' AND available_at <= now()) OR (delivery_status = 'dispatching' AND lease_until <= now())) ORDER BY available_at,created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE content_report_restriction_jobs j SET delivery_status = 'dispatching',attempts = attempts + 1,lease_until = now() + make_interval(secs => $2),updated_at = now() FROM claimed WHERE j.report_id = claimed.report_id RETURNING j.report_id,j.content_id",
        )
        .bind(self.batch_size)
        .bind(self.lease_seconds)
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(rows
            .into_iter()
            .map(|(report_id, content_id)| RestrictionJob {
                report_id,
                content_id,
            })
            .collect())
    }

    async fn mark_delivered(&self, job: &RestrictionJob) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE content_report_restriction_jobs SET delivery_status = 'delivered',lease_until = NULL,delivered_at = now(),last_error = NULL,updated_at = now() WHERE report_id = $1 AND delivery_status = 'dispatching'",
        )
        .bind(&job.report_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_failed(&self, job: &RestrictionJob, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE content_report_restriction_jobs SET delivery_status = CASE WHEN attempts >= $2 THEN 'dead' ELSE 'pending' END,lease_until = NULL,available_at = now() + make_interval(secs => LEAST(300, CAST(power(2, LEAST(attempts, 8)) AS INTEGER)) + floor(random() * 3)::INTEGER),last_error = left($3,2000),updated_at = now() WHERE report_id = $1 AND delivery_status = 'dispatching'",
        )
        .bind(&job.report_id)
        .bind(MAX_DELIVERY_ATTEMPTS)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

struct GrpcRestrictionTarget {
    bbs_link: bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient<tonic::transport::Channel>,
    service_auth_token: Option<MetadataValue<Ascii>>,
}

impl GrpcRestrictionTarget {
    async fn connect(
        bbs_link_url: String,
        service_auth_token: Option<MetadataValue<Ascii>>,
    ) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            bbs_link: bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient::connect(
                bbs_link_url,
            )
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
impl RestrictionTarget for GrpcRestrictionTarget {
    async fn restrict(&self, job: &RestrictionJob) -> Result<DeliveryDisposition, TargetError> {
        let mut bbs_link = self.bbs_link.clone();
        bbs_link
            .restrict(bbs_link_request(
                bookway_bbs_link::api::pb::RestrictRequest {
                    content_id: job.content_id.clone(),
                },
                self.service_auth_token.as_ref(),
            ))
            .await
            .map_err(TargetError::BbsLink)?;
        match bbs_link
            .get_public(bbs_link_request(
                bookway_bbs_link::api::pb::IdRequest {
                    id: job.content_id.clone(),
                },
                self.service_auth_token.as_ref(),
            ))
            .await
        {
            Err(status) if status.code() == tonic::Code::NotFound => {
                Ok(DeliveryDisposition::Restricted)
            }
            Ok(_) => Ok(DeliveryDisposition::Deferred),
            Err(status) => Err(TargetError::BbsLink(status)),
        }
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
    T: RestrictionTarget + 'static,
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

    async fn dispatch(&self, job: RestrictionJob) -> Result<(), DispatcherError> {
        let delivery = tokio::time::timeout(self.request_timeout, self.target.restrict(&job)).await;
        match delivery {
            Ok(Ok(DeliveryDisposition::Restricted)) => {
                self.repository.mark_delivered(&job).await?;
                tracing::debug!(report_id = %job.report_id, "content restriction delivered");
            }
            Ok(Ok(DeliveryDisposition::Deferred)) => {
                self.repository
                    .mark_failed(
                        &job,
                        "waiting for public content read to become unavailable",
                    )
                    .await?;
                tracing::debug!(report_id = %job.report_id, "content restriction deferred");
            }
            Ok(Err(error)) => {
                self.repository
                    .mark_failed(&job, &error.to_string())
                    .await?;
                tracing::warn!(report_id = %job.report_id, error = %error, "content restriction failed");
            }
            Err(_) => {
                let error = TargetError::Timeout(self.request_timeout.as_millis() as u64);
                self.repository
                    .mark_failed(&job, &error.to_string())
                    .await?;
                tracing::warn!(report_id = %job.report_id, error = %error, "content restriction timed out");
            }
        }
        Ok(())
    }
}

struct Config {
    bbs_link_url: String,
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
            service_auth_token: service_auth_token()?,
            batch_size: env_number("REPORT_RESTRICTION_BATCH_SIZE", 100_i64)?.clamp(1, 1_000),
            concurrency: env_number("REPORT_RESTRICTION_CONCURRENCY", 16_usize)?.clamp(1, 128),
            lease_seconds: env_number("REPORT_RESTRICTION_LEASE_SECONDS", 30_i32)?.clamp(5, 300),
            request_timeout: Duration::from_millis(
                env_number("REPORT_RESTRICTION_REQUEST_TIMEOUT_MS", 3_000_u64)?.clamp(100, 30_000),
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
    bookway_runtime::init_tracing("content-report-restriction-dispatcher");
    let config = Config::from_env()?;
    let repository = Arc::new(PostgresJobRepository::new(
        bookway_data::postgres_pool().await?,
        config.batch_size,
        config.lease_seconds,
    ));
    let target = Arc::new(
        GrpcRestrictionTarget::connect(config.bbs_link_url, config.service_auth_token).await?,
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
            Ok(count) => tracing::debug!(count, "content restriction jobs dispatched"),
            Err(error) => {
                tracing::error!(%error, "content restriction dispatcher iteration failed");
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
        pending: Mutex<Vec<RestrictionJob>>,
        delivered: Mutex<Vec<String>>,
        failed: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl JobRepository for MemoryRepository {
        async fn claim(&self) -> Result<Vec<RestrictionJob>, sqlx::Error> {
            Ok(std::mem::take(
                &mut *self.pending.lock().expect("pending lock"),
            ))
        }

        async fn mark_delivered(&self, job: &RestrictionJob) -> Result<(), sqlx::Error> {
            self.delivered
                .lock()
                .expect("delivered lock")
                .push(job.report_id.clone());
            Ok(())
        }

        async fn mark_failed(&self, job: &RestrictionJob, error: &str) -> Result<(), sqlx::Error> {
            self.failed
                .lock()
                .expect("failed lock")
                .push((job.report_id.clone(), error.to_string()));
            Ok(())
        }
    }

    struct RecordingTarget {
        disposition: DeliveryDisposition,
        received: Mutex<Vec<RestrictionJob>>,
    }

    #[async_trait]
    impl RestrictionTarget for RecordingTarget {
        async fn restrict(&self, job: &RestrictionJob) -> Result<DeliveryDisposition, TargetError> {
            self.received
                .lock()
                .expect("received lock")
                .push(job.clone());
            Ok(self.disposition)
        }
    }

    fn job() -> RestrictionJob {
        RestrictionJob {
            report_id: "report-1".to_string(),
            content_id: "content-1".to_string(),
        }
    }

    #[tokio::test]
    async fn confirmed_restrictions_are_acknowledged() {
        let repository = Arc::new(MemoryRepository {
            pending: Mutex::new(vec![job()]),
            ..Default::default()
        });
        let target = Arc::new(RecordingTarget {
            disposition: DeliveryDisposition::Restricted,
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
            ["report-1"]
        );
        assert!(repository.failed.lock().expect("failed lock").is_empty());
        assert_eq!(target.received.lock().expect("received lock").len(), 1);
    }

    #[tokio::test]
    async fn unconfirmed_restrictions_remain_retryable() {
        let repository = Arc::new(MemoryRepository {
            pending: Mutex::new(vec![job()]),
            ..Default::default()
        });
        let target = Arc::new(RecordingTarget {
            disposition: DeliveryDisposition::Deferred,
            received: Mutex::new(Vec::new()),
        });
        let dispatcher = Dispatcher::new(repository.clone(), target, 1, Duration::from_secs(1));

        assert_eq!(dispatcher.run_once().await.expect("dispatch"), 1);
        assert!(
            repository
                .delivered
                .lock()
                .expect("delivered lock")
                .is_empty()
        );
        assert_eq!(repository.failed.lock().expect("failed lock").len(), 1);
    }

    #[test]
    fn restriction_request_carries_the_worker_service_token() {
        let token: MetadataValue<Ascii> = "dispatcher-token".try_into().expect("valid token");
        let request = bbs_link_request(
            bookway_bbs_link::api::pb::RestrictRequest {
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
