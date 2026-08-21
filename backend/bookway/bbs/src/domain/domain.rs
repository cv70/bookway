use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        BbsDao, CachedBbsDao, MemoryBbsDao, PostgresBbsDao,
        DaoError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum BbsError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Dao(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn BbsDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let dao: Arc<dyn BbsDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryBbsDao::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresBbsDao::new(
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
            dao: Arc::new(CachedBbsDao::new(dao, redis)),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, dao: Arc<dyn BbsDao>) -> Self {
        Self { config, dao }
    }
}
