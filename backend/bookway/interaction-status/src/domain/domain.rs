use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        CachedInteractionStatusRepository, InteractionStatusRepository,
        MemoryInteractionStatusRepository, PostgresInteractionStatusRepository, RepositoryError,
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
            bookway_data::StorageMode::Memory => {
                Arc::new(MemoryInteractionStatusRepository::seeded())
            }
            bookway_data::StorageMode::Postgres => Arc::new(
                PostgresInteractionStatusRepository::new(bookway_data::postgres_pool().await?),
            ),
        };
        let redis = match bookway_data::redis_connection().await {
            Ok(redis) => redis,
            Err(error) => {
                tracing::warn!(%error, "redis unavailable at startup; interaction context cache disabled");
                None
            }
        };
        Ok(Self {
            config,
            repository: Arc::new(CachedInteractionStatusRepository::new(repository, redis)),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_repository(
        config: Config,
        repository: Arc<dyn InteractionStatusRepository>,
    ) -> Self {
        Self { config, repository }
    }
}
