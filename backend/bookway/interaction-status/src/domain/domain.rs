use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        CachedInteractionStatusDao, DaoError, InteractionStatusDao, MemoryInteractionStatusDao,
        PostgresInteractionStatusDao,
    },
};

#[derive(Debug, Error)]
pub(crate) enum InteractionStatusError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn InteractionStatusDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let dao: Arc<dyn InteractionStatusDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryInteractionStatusDao::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresInteractionStatusDao::new(
                bookway_data::postgres_pool().await?,
            )),
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
            dao: Arc::new(CachedInteractionStatusDao::new(dao, redis)),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, dao: Arc<dyn InteractionStatusDao>) -> Self {
        Self { config, dao }
    }
}
