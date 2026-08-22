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
pub(crate) struct CommunityNotificationJob {
    pub(crate) source_id: String,
    pub(crate) recipient_user_id: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) data: Value,
}

#[path = "community_notification_job_dao.rs"]
mod community_notification_job_dao;
pub(crate) use community_notification_job_dao::CommunityNotificationJobDao;
