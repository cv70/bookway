pub(crate) mod creator;

use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        CreatorDao, MemoryCreatorDao, PostgresCreatorDao, DaoError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum CreatorError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Dao(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) Dao: Arc<dyn CreatorDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let Dao: Arc<dyn CreatorDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCreatorDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCreatorDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, Dao })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, Dao: Arc<dyn CreatorDao>) -> Self {
        Self { config, Dao }
    }
}
