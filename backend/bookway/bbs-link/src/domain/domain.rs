use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        ContentAuditor, ContentRepository, GrpcContentAuditor, LocalContentAuditor,
        MemoryContentRepository, PostgresContentRepository, RepositoryError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum ContentError {
    #[error("{0}")]
    Validation(String),
    #[error("content belongs to another author")]
    Forbidden,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("content audit unavailable: {0}")]
    Audit(String),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn ContentRepository>,
    pub(crate) auditor: Arc<dyn ContentAuditor>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: Arc<dyn ContentRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryContentRepository::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresContentRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let auditor: Arc<dyn ContentAuditor> = match config.content_audit_grpc_url.clone() {
            Some(url) => Arc::new(GrpcContentAuditor::connect(url).await.map_err(|error| {
                bookway_data::DataError::InvalidPoolSetting {
                    key: "CONTENT_AUDIT_GRPC_URL",
                    value: error.to_string(),
                }
            })?),
            _ => Arc::new(LocalContentAuditor),
        };
        Ok(Self {
            config,
            repository,
            auditor,
        })
    }

    pub(crate) fn from_repositories(
        config: Config,
        repository: Arc<dyn ContentRepository>,
        auditor: Arc<dyn ContentAuditor>,
    ) -> Self {
        Self {
            config,
            repository,
            auditor,
        }
    }

    pub(crate) fn with_auditor(mut self, auditor: Arc<dyn ContentAuditor>) -> Self {
        self.auditor = auditor;
        self
    }
}
