pub(crate) mod creator;

use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{CreatorDao, DaoError, MemoryCreatorDao, PostgresCreatorDao},
};

#[derive(Debug, Error)]
pub(crate) enum CreatorError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn CreatorDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let dao: Arc<dyn CreatorDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCreatorDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCreatorDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, dao })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, dao: Arc<dyn CreatorDao>) -> Self {
        Self { config, dao }
    }
}
