use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum UpstreamError {
    #[error("{service} grpc request failed: {message}")]
    Transport {
        service: &'static str,
        message: String,
    },
    #[error("{service} grpc request failed with {code:?}: {message}")]
    Grpc {
        service: &'static str,
        code: tonic::Code,
        message: String,
    },
}

/// Gateway owns this cross-service work item because it resolves recipients
/// while coordinating interactions owned by other services.
#[derive(Clone)]
pub(crate) struct CommunityNotificationJobRepository {
    pool: PgPool,
}

pub(crate) struct CommunityNotificationJob {
    pub(crate) source_id: String,
    pub(crate) recipient_user_id: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) data: Value,
}

impl CommunityNotificationJobRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn enqueue(&self, job: CommunityNotificationJob) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO community_notification_jobs (source_id,recipient_user_id,title,body,data) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (source_id) DO NOTHING",
        )
        .bind(job.source_id)
        .bind(job.recipient_user_id)
        .bind(job.title)
        .bind(job.body)
        .bind(job.data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
