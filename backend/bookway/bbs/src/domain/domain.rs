use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        BbsRepository, CachedBbsRepository, MemoryBbsRepository, PostgresBbsRepository,
        RepositoryError,
    },
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
        let redis = match bookway_data::redis_connection().await {
            Ok(redis) => redis,
            Err(error) => {
                tracing::warn!(%error, "redis unavailable at startup; bbs relationship cache disabled");
                None
            }
        };
        Ok(Self {
            config,
            repository: Arc::new(CachedBbsRepository::new(repository, redis)),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_repository(config: Config, repository: Arc<dyn BbsRepository>) -> Self {
        Self { config, repository }
    }
}
