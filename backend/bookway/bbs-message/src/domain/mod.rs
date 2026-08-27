pub(crate) mod message;

use std::sync::Arc;

use bookway_bbs_api::pb::{self as bbs_pb, bbs_client::BbsClient};
use bookway_content_audit_api::pb::{self as audit_pb, content_audit_client::ContentAuditClient};
use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{DaoError, MemoryMessageDao, MessageDao, PostgresMessageDao},
};

#[derive(Debug, Error)]
pub(crate) enum MessageError {
    #[error("{0}")]
    Validation(String),
    #[error("direct messages are unavailable for this relationship")]
    Blocked,
    #[error("the recipient is not accepting direct messages")]
    RecipientUnavailable,
    #[error("the sender is restricted from sending direct messages")]
    SenderRestricted,
    #[error("the message requires safety review before it can be delivered")]
    UnderReview,
    #[error("the message cannot be delivered under community safety rules")]
    Restricted,
    #[error("message safety audit unavailable: {0}")]
    Audit(String),
    #[error("upstream social dependency failed: {0}")]
    Upstream(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn MessageDao>,
    pub(crate) bbs: BbsClient<tonic::transport::Channel>,
    pub(crate) content_audit: Option<ContentAuditClient<tonic::transport::Channel>>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_mode = bookway_data::storage_mode()?;
        let dao: Arc<dyn MessageDao> = match storage_mode {
            bookway_data::StorageMode::Memory => Arc::new(MemoryMessageDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresMessageDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let content_audit = match config.content_audit_grpc_url.clone() {
            Some(url) => Some(ContentAuditClient::new(
                bookway_runtime::grpc_channel(&url).await?,
            )),
            None if storage_mode == bookway_data::StorageMode::Postgres => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "CONTENT_AUDIT_GRPC_URL is required when STORAGE_MODE=postgres",
                )
                .into());
            }
            None => None,
        };
        Ok(Self {
            bbs: BbsClient::new(bookway_runtime::grpc_channel(&config.bbs_grpc_url).await?),
            content_audit,
            config,
            dao,
        })
    }

    pub(crate) async fn social_context(
        &self,
        user_id: String,
    ) -> Result<bbs_pb::SocialContext, MessageError> {
        let mut bbs = self.bbs.clone();
        bbs.context(
            bookway_runtime::grpc_service_request(bbs_pb::ContextRequest {
                user_id,
                post_ids: Vec::new(),
            })
            .map_err(|error| MessageError::Upstream(error.to_string()))?,
        )
        .await
        .map(tonic::Response::into_inner)
        .map_err(|error| MessageError::Upstream(error.to_string()))
    }

    pub(crate) async fn audit_message(
        &self,
        content_id: String,
        body: String,
    ) -> Result<audit_pb::AuditResponse, MessageError> {
        let Some(mut content_audit) = self.content_audit.clone() else {
            // This fallback exists solely for no-dependency local development.
            // PostgreSQL mode fails during startup when no audit client is configured.
            return Ok(audit_pb::AuditResponse {
                decision: audit_pb::AuditDecision::Approved as i32,
                risk_score: 0.0,
                reasons: Vec::new(),
                provider: "local-development".to_string(),
            });
        };
        content_audit
            .audit(
                bookway_runtime::grpc_service_request(audit_pb::AuditRequest {
                    content_id,
                    version: 1,
                    title: "私信".to_string(),
                    body,
                })
                .map_err(|error| MessageError::Audit(error.to_string()))?,
            )
            .await
            .map(tonic::Response::into_inner)
            .map_err(|error| MessageError::Audit(error.to_string()))
    }
}
