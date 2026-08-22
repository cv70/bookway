use std::{env, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::{StreamExt, stream};
use sqlx::PgPool;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RouteParticipationIntent {
    user_id: String,
    route_id: String,
    private_journey_id: Option<String>,
    desired_active: bool,
    version: i64,
}

#[derive(Debug, Error)]
enum ReconcileError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
enum TargetError {
    #[error("bbs request failed: {0}")]
    Grpc(#[from] tonic::Status),
    #[error("bbs service authentication failed: {0}")]
    ServiceAuth(#[from] bookway_runtime::GrpcServiceAuthError),
    #[error("bbs request timed out after {0}ms")]
    Timeout(u64),
    #[error("invalid route participation intent version: {0}")]
    InvalidVersion(i64),
}

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
}

#[async_trait]
trait IntentDao: Send + Sync {
    async fn claim(&self) -> Result<Vec<RouteParticipationIntent>, sqlx::Error>;
    async fn mark_applied(&self, intent: &RouteParticipationIntent) -> Result<(), sqlx::Error>;
    async fn mark_failed(
        &self,
        intent: &RouteParticipationIntent,
        error: &str,
    ) -> Result<(), sqlx::Error>;
}

#[async_trait]
trait ParticipationTarget: Send + Sync {
    async fn apply(&self, intent: &RouteParticipationIntent) -> Result<(), TargetError>;
}

struct PostgresIntentDao {
    pool: PgPool,
    batch_size: i64,
    lease_seconds: i32,
}

impl PostgresIntentDao {
    fn new(pool: PgPool, batch_size: i64, lease_seconds: i32) -> Self {
        Self {
            pool,
            batch_size,
            lease_seconds,
        }
    }
}

#[async_trait]
impl IntentDao for PostgresIntentDao {
    async fn claim(&self) -> Result<Vec<RouteParticipationIntent>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (String, String, Option<String>, bool, i64)>(
            "WITH claimed AS (SELECT user_id, route_id FROM route_participation_intents WHERE applied_version < version AND available_at <= now() AND (lease_until IS NULL OR lease_until <= now()) ORDER BY available_at, updated_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE route_participation_intents i SET lease_until = now() + make_interval(secs => $2), attempts = attempts + 1, last_attempt_at = now() FROM claimed WHERE i.user_id = claimed.user_id AND i.route_id = claimed.route_id RETURNING i.user_id, i.route_id, i.private_journey_id, i.desired_active, i.version",
        )
        .bind(self.batch_size)
        .bind(self.lease_seconds)
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(rows
            .into_iter()
            .map(
                |(user_id, route_id, private_journey_id, desired_active, version)| {
                    RouteParticipationIntent {
                        user_id,
                        route_id,
                        private_journey_id,
                        desired_active,
                        version,
                    }
                },
            )
            .collect())
    }

    async fn mark_applied(&self, intent: &RouteParticipationIntent) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE route_participation_intents SET applied_version = GREATEST(applied_version, $3), lease_until = NULL, attempts = CASE WHEN version = $3 THEN 0 ELSE attempts END, available_at = now(), last_error = CASE WHEN version = $3 THEN NULL ELSE last_error END, updated_at = now() WHERE user_id = $1 AND route_id = $2",
        )
        .bind(&intent.user_id)
        .bind(&intent.route_id)
        .bind(intent.version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_failed(
        &self,
        intent: &RouteParticipationIntent,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE route_participation_intents SET lease_until = NULL, available_at = CASE WHEN version = $3 THEN now() + make_interval(secs => LEAST(300, CAST(power(2, LEAST(attempts, 8)) AS INTEGER)) + floor(random() * 3)::INTEGER) ELSE now() END, last_error = left($4, 2000), updated_at = now() WHERE user_id = $1 AND route_id = $2 AND applied_version < version",
        )
        .bind(&intent.user_id)
        .bind(&intent.route_id)
        .bind(intent.version)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

struct BbsGrpcTarget {
    client: bookway_bbs_api::pb::bbs_client::BbsClient<tonic::transport::Channel>,
}

impl BbsGrpcTarget {
    async fn connect(address: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: bookway_bbs_api::pb::bbs_client::BbsClient::connect(address).await?,
        })
    }
}

#[async_trait]
impl ParticipationTarget for BbsGrpcTarget {
    async fn apply(&self, intent: &RouteParticipationIntent) -> Result<(), TargetError> {
        let mut client = self.client.clone();
        client
            .set_route_participation(bookway_runtime::grpc_service_request(
                bookway_bbs_api::pb::RouteParticipationRequest {
                    user_id: intent.user_id.clone(),
                    route_id: intent.route_id.clone(),
                    active: intent.desired_active,
                    private_journey_id: intent
                        .desired_active
                        .then(|| intent.private_journey_id.clone())
                        .flatten(),
                    intent_version: Some(
                        u64::try_from(intent.version)
                            .map_err(|_| TargetError::InvalidVersion(intent.version))?,
                    ),
                },
            )?)
            .await?;
        Ok(())
    }
}

struct Reconciler<R, T> {
    dao: Arc<R>,
    target: Arc<T>,
    concurrency: usize,
    request_timeout: Duration,
}

impl<R, T> Reconciler<R, T>
where
    R: IntentDao + 'static,
    T: ParticipationTarget + 'static,
{
    fn new(dao: Arc<R>, target: Arc<T>, concurrency: usize, request_timeout: Duration) -> Self {
        Self {
            dao,
            target,
            concurrency,
            request_timeout,
        }
    }

    async fn run_once(&self) -> Result<usize, ReconcileError> {
        let intents = self.dao.claim().await?;
        let count = intents.len();
        let results = stream::iter(intents)
            .map(|intent| self.reconcile(intent))
            .buffer_unordered(self.concurrency)
            .collect::<Vec<_>>()
            .await;
        for result in results {
            result?;
        }
        Ok(count)
    }

    async fn reconcile(&self, intent: RouteParticipationIntent) -> Result<(), ReconcileError> {
        let result = tokio::time::timeout(self.request_timeout, self.target.apply(&intent)).await;
        let failure = match result {
            Ok(Ok(())) => {
                self.dao.mark_applied(&intent).await?;
                tracing::debug!(
                    user_id = %intent.user_id,
                    route_id = %intent.route_id,
                    version = intent.version,
                    desired_active = intent.desired_active,
                    "route participation intent applied"
                );
                return Ok(());
            }
            Ok(Err(error)) => error,
            Err(_) => TargetError::Timeout(self.request_timeout.as_millis() as u64),
        };
        self.dao.mark_failed(&intent, &failure.to_string()).await?;
        tracing::warn!(
            user_id = %intent.user_id,
            route_id = %intent.route_id,
            version = intent.version,
            error = %failure,
            "route participation reconciliation failed"
        );
        Ok(())
    }
}

struct Config {
    bbs_url: String,
    batch_size: i64,
    concurrency: usize,
    lease_seconds: i32,
    request_timeout: Duration,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bbs_url: env::var("BBS_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18002".to_string()),
            batch_size: env_number("ROUTE_RECONCILE_BATCH_SIZE", 100_i64)?.clamp(1, 1000),
            concurrency: env_number("ROUTE_RECONCILE_CONCURRENCY", 16_usize)?.clamp(1, 128),
            lease_seconds: env_number("ROUTE_RECONCILE_LEASE_SECONDS", 30_i32)?.clamp(5, 300),
            request_timeout: Duration::from_millis(
                env_number("ROUTE_RECONCILE_REQUEST_TIMEOUT_MS", 3000_u64)?.clamp(100, 30_000),
            ),
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("route-participation-reconciler");
    let config = Config::from_env()?;
    let dao = Arc::new(PostgresIntentDao::new(
        bookway_data::postgres_pool().await?,
        config.batch_size,
        config.lease_seconds,
    ));
    let target = Arc::new(BbsGrpcTarget::connect(config.bbs_url).await?);
    let reconciler = Reconciler::new(dao, target, config.concurrency, config.request_timeout);
    loop {
        match reconciler.run_once().await {
            Ok(0) => tokio::time::sleep(Duration::from_millis(500)).await,
            Ok(count) => tracing::debug!(count, "route participation intents reconciled"),
            Err(error) => {
                tracing::error!(%error, "route participation reconciler iteration failed");
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
    struct MemoryDao {
        pending: Mutex<Vec<RouteParticipationIntent>>,
        applied: Mutex<Vec<(String, i64)>>,
        failed: Mutex<Vec<(String, i64, String)>>,
    }

    #[async_trait]
    impl IntentDao for MemoryDao {
        async fn claim(&self) -> Result<Vec<RouteParticipationIntent>, sqlx::Error> {
            Ok(std::mem::take(
                &mut *self.pending.lock().expect("pending lock"),
            ))
        }

        async fn mark_applied(&self, intent: &RouteParticipationIntent) -> Result<(), sqlx::Error> {
            self.applied
                .lock()
                .expect("applied lock")
                .push((intent.route_id.clone(), intent.version));
            Ok(())
        }

        async fn mark_failed(
            &self,
            intent: &RouteParticipationIntent,
            error: &str,
        ) -> Result<(), sqlx::Error> {
            self.failed.lock().expect("failed lock").push((
                intent.route_id.clone(),
                intent.version,
                error.to_string(),
            ));
            Ok(())
        }
    }

    struct RecordingTarget {
        fail: bool,
        calls: Mutex<Vec<RouteParticipationIntent>>,
    }

    #[async_trait]
    impl ParticipationTarget for RecordingTarget {
        async fn apply(&self, intent: &RouteParticipationIntent) -> Result<(), TargetError> {
            self.calls.lock().expect("calls lock").push(intent.clone());
            if self.fail {
                return Err(TargetError::Grpc(tonic::Status::unavailable("bbs down")));
            }
            Ok(())
        }
    }

    fn intent(active: bool, version: i64) -> RouteParticipationIntent {
        RouteParticipationIntent {
            user_id: "user-a".to_string(),
            route_id: "route-a".to_string(),
            private_journey_id: active.then(|| "journey-a".to_string()),
            desired_active: active,
            version,
        }
    }

    #[tokio::test]
    async fn applies_and_acknowledges_the_exact_claimed_version() {
        let dao = Arc::new(MemoryDao::default());
        dao.pending
            .lock()
            .expect("pending lock")
            .push(intent(false, 7));
        let target = Arc::new(RecordingTarget {
            fail: false,
            calls: Mutex::new(Vec::new()),
        });
        let reconciler = Reconciler::new(dao.clone(), target.clone(), 4, Duration::from_secs(1));

        assert_eq!(reconciler.run_once().await.expect("run once"), 1);
        assert_eq!(
            *dao.applied.lock().expect("applied lock"),
            vec![("route-a".to_string(), 7)]
        );
        assert!(!target.calls.lock().expect("calls lock")[0].desired_active);
        assert!(dao.failed.lock().expect("failed lock").is_empty());
    }

    #[tokio::test]
    async fn records_target_failures_without_acknowledging_the_intent() {
        let dao = Arc::new(MemoryDao::default());
        dao.pending
            .lock()
            .expect("pending lock")
            .push(intent(true, 3));
        let target = Arc::new(RecordingTarget {
            fail: true,
            calls: Mutex::new(Vec::new()),
        });
        let reconciler = Reconciler::new(dao.clone(), target, 4, Duration::from_secs(1));

        assert_eq!(reconciler.run_once().await.expect("run once"), 1);
        assert!(dao.applied.lock().expect("applied lock").is_empty());
        let failed = dao.failed.lock().expect("failed lock");
        assert_eq!(failed[0].0, "route-a");
        assert_eq!(failed[0].1, 3);
        assert!(failed[0].2.contains("bbs down"));
    }
}
