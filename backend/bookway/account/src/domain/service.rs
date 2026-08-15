use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        AccountRepository, MemoryAccountRepository, PostgresAccountRepository, RepositoryError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum AccountError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: Arc<dyn AccountRepository>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: Arc<dyn AccountRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryAccountRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresAccountRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }

    #[cfg(test)]
    pub(crate) fn from_repository(config: Config, repository: Arc<dyn AccountRepository>) -> Self {
        Self { config, repository }
    }
}
