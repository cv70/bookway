use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        AccountDao, MemoryAccountDao, PostgresAccountDao, DaoError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum AccountError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Dao(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) Dao: Arc<dyn AccountDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let Dao: Arc<dyn AccountDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryAccountDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresAccountDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, Dao })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, Dao: Arc<dyn AccountDao>) -> Self {
        Self { config, Dao }
    }
}
