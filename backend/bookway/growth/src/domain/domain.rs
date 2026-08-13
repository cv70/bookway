use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        GrowthRepository, MemoryGrowthRepository, PostgresGrowthRepository, RepositoryError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum GrowthError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn GrowthRepository>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: Arc<dyn GrowthRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryGrowthRepository::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresGrowthRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }

    pub(crate) fn from_repository(config: Config, repository: Arc<dyn GrowthRepository>) -> Self {
        Self { config, repository }
    }
}
