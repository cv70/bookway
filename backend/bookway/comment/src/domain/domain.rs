use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        CommentRepository, MemoryCommentRepository, PostgresCommentRepository, RepositoryError,
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
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: Arc<dyn CommentRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCommentRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCommentRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }

    pub(crate) fn from_repository(config: Config, repository: Arc<dyn CommentRepository>) -> Self {
        Self { config, repository }
    }
}
