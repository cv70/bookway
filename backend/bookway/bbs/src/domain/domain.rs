use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{BbsRepository, MemoryBbsRepository, PostgresBbsRepository, RepositoryError},
};

#[derive(Debug, Error)]
pub(crate) enum BbsError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn BbsRepository>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: Arc<dyn BbsRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryBbsRepository::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresBbsRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }

    pub(crate) fn from_repository(config: Config, repository: Arc<dyn BbsRepository>) -> Self {
        Self { config, repository }
    }
}
