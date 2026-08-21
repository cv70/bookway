use crate::api::pb;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("account profile {0} was not found")]
    NotFound(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[async_trait]
pub(crate) trait AccountDao: Send + Sync {
    async fn get_or_create(&self, user_id: &str) -> Result<pb::AccountProfile, DaoError>;
    async fn update(
        &self,
        user_id: &str,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, DaoError>;
}

mod account_profile;
mod memory_account_dao;
pub(crate) use memory_account_dao::MemoryAccountDao;
mod postgres_account_dao;
pub(crate) use postgres_account_dao::PostgresAccountDao;
