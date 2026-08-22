use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{DaoError, GrowthDao, MemoryGrowthDao, PostgresGrowthDao},
};

#[derive(Debug, Error)]
pub(crate) enum GrowthError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn GrowthDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let dao: Arc<dyn GrowthDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryGrowthDao::seeded()),
            bookway_data::StorageMode::Postgres => {
                Arc::new(PostgresGrowthDao::new(bookway_data::postgres_pool().await?))
            }
        };
        Ok(Self { config, dao })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, dao: Arc<dyn GrowthDao>) -> Self {
        Self { config, dao }
    }
}
