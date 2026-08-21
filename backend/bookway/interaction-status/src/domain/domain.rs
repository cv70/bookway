use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        CachedInteractionStatusDao, InteractionStatusDao,
        MemoryInteractionStatusDao, PostgresInteractionStatusDao, DaoError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum InteractionStatusError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Dao(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) Dao: Arc<dyn InteractionStatusDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let Dao: Arc<dyn InteractionStatusDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => {
                Arc::new(MemoryInteractionStatusDao::seeded())
            }
            bookway_data::StorageMode::Postgres => Arc::new(
                PostgresInteractionStatusDao::new(bookway_data::postgres_pool().await?),
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
            Dao: Arc::new(CachedInteractionStatusDao::new(Dao, redis)),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(
        config: Config,
        Dao: Arc<dyn InteractionStatusDao>,
    ) -> Self {
        Self { config, Dao }
    }
}
