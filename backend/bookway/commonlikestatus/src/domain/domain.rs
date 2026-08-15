use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        LikeStatusRepository, MemoryLikeStatusRepository, PostgresLikeStatusRepository,
        RepositoryError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum LikeStatusError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn LikeStatusRepository>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: Arc<dyn LikeStatusRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryLikeStatusRepository::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresLikeStatusRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }

    #[cfg(test)]
    pub(crate) fn from_repository(
        config: Config,
        repository: Arc<dyn LikeStatusRepository>,
    ) -> Self {
        Self { config, repository }
    }
}
