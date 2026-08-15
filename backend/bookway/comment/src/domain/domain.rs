use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        CommentAuditor, CommentRepository, GrpcCommentAuditor, LocalCommentAuditor,
        MemoryCommentRepository, PostgresCommentRepository, RepositoryError,
        UnavailableCommentAuditor,
    },
};

#[derive(Debug, Error)]
pub(crate) enum CommentError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn CommentRepository>,
    pub(crate) auditor: Arc<dyn CommentAuditor>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_mode = bookway_data::storage_mode()?;
        let repository: Arc<dyn CommentRepository> = match storage_mode {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCommentRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCommentRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let auditor: Arc<dyn CommentAuditor> = match config.content_audit_grpc_url.clone() {
            Some(url) => Arc::new(GrpcCommentAuditor::connect(url).await?),
            None if storage_mode == bookway_data::StorageMode::Memory => {
                Arc::new(LocalCommentAuditor)
            }
            None => Arc::new(UnavailableCommentAuditor),
        };
        Ok(Self {
            config,
            repository,
            auditor,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_repository(config: Config, repository: Arc<dyn CommentRepository>) -> Self {
        Self {
            config,
            repository,
            auditor: Arc::new(LocalCommentAuditor),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_auditor(mut self, auditor: Arc<dyn CommentAuditor>) -> Self {
        self.auditor = auditor;
        self
    }
}
