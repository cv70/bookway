use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{AccountDao, DaoError, MemoryAccountDao, PostgresAccountDao},
};

#[derive(Debug, Error)]
pub(crate) enum AccountError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) dao: Arc<dyn AccountDao>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let dao: Arc<dyn AccountDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryAccountDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresAccountDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, dao })
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, dao: Arc<dyn AccountDao>) -> Self {
        Self { config, dao }
    }
}
