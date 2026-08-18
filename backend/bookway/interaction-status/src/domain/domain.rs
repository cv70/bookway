use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        InteractionStatusRepository, MemoryInteractionStatusRepository, PostgresInteractionStatusRepository,
        RepositoryError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum InteractionStatusError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn InteractionStatusRepository>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: Arc<dyn InteractionStatusRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryInteractionStatusRepository::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresInteractionStatusRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }

    #[cfg(test)]
    pub(crate) fn from_repository(
        config: Config,
        repository: Arc<dyn InteractionStatusRepository>,
    ) -> Self {
        Self { config, repository }
    }
}
