use std::sync::Arc;

use bookway_content_audit_api::pb::{self as audit_pb, content_audit_client::ContentAuditClient};
use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{CommentDao, DaoError, MemoryCommentDao, PostgresCommentDao},
};

#[derive(Debug, Error)]
pub(crate) enum CommentError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn CommentDao>,
    pub(crate) content_audit: Option<ContentAuditClient<tonic::transport::Channel>>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_mode = bookway_data::storage_mode()?;
        let dao: Arc<dyn CommentDao> = match storage_mode {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCommentDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCommentDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let content_audit = match config.content_audit_grpc_url.clone() {
            Some(url) => Some(ContentAuditClient::connect(url).await?),
            None => None,
        };
        Ok(Self {
            config,
            dao,
            content_audit,
        })
    }

    pub(crate) async fn audit(
        &self,
        request: audit_pb::AuditRequest,
    ) -> Result<audit_pb::AuditResponse, String> {
        let Some(mut client) = self.content_audit.clone() else {
            return Ok(audit_pb::AuditResponse {
                decision: audit_pb::AuditDecision::Approved as i32,
                risk_score: 0.0,
                reasons: Vec::new(),
                provider: "local-development".to_string(),
            });
        };
        client
            .audit(
                bookway_runtime::grpc_service_request(request)
                    .map_err(|error| error.to_string())?,
            )
            .await
            .map(tonic::Response::into_inner)
            .map_err(|error| error.to_string())
    }
}
