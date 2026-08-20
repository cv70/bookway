use crate::api::pb;
use crate::{
    Config,
    datasource::{
        InventoryRepository, MemoryInventoryRepository, PostgresInventoryRepository,
        RedisInventoryRepository, RepositoryError,
    },
};
use std::sync::Arc;
use thiserror::Error;
#[derive(Debug, Error)]
pub(crate) enum InventoryError {
    #[error("{0}")]
    Validation(String),
    #[error("inventory or reservation {0} was not found")]
    NotFound(String),
    #[error("insufficient inventory: {0}")]
    Insufficient(String),
    #[error("reservation conflict: {0}")]
    Conflict(String),
    #[error("inventory operation failed: {0}")]
    Repository(String),
}
#[derive(Clone)]
pub struct Domain {
    config: Config,
    repository: Arc<dyn InventoryRepository>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: Arc<dyn InventoryRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryInventoryRepository::default()),
            bookway_data::StorageMode::Postgres => {
                let postgres =
                    PostgresInventoryRepository::new(bookway_data::postgres_pool().await?);
                match bookway_data::redis_connection().await {
                    Ok(Some(redis)) => Arc::new(RedisInventoryRepository::new(postgres, redis)),
                    Ok(None) => Arc::new(postgres),
                    Err(error) => {
                        tracing::warn!(%error, "redis unavailable at startup; inventory uses PostgreSQL reservations");
                        Arc::new(postgres)
                    }
                }
            }
        };
        Ok(Self { config, repository })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn set_stock(
        &self,
        request: pb::SetStockRequest,
    ) -> Result<pb::InventoryItem, InventoryError> {
        if request.sku_id.trim().is_empty() || request.available < 0 {
            return Err(InventoryError::Validation(
                "sku_id and a non-negative available count are required".to_string(),
            ));
        }
        self.repository.set_stock(request).await.map_err(repo_error)
    }
    pub(crate) async fn stock(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::InventoryItem, InventoryError> {
        if request.id.trim().is_empty() {
            return Err(InventoryError::Validation("sku id is required".to_string()));
        }
        self.repository.stock(&request.id).await.map_err(repo_error)
    }
    pub(crate) async fn reserve(
        &self,
        mut request: pb::ReserveRequest,
    ) -> Result<pb::Reservation, InventoryError> {
        if request.reservation_id.trim().is_empty()
            || request.items.is_empty()
            || request.items.len() > 100
            || request
                .items
                .iter()
                .any(|item| item.sku_id.trim().is_empty() || item.quantity == 0)
        {
            return Err(InventoryError::Validation(
                "reservation_id and 1-100 positive SKU quantities are required".to_string(),
            ));
        }
        if request
            .items
            .iter()
            .map(|item| &item.sku_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != request.items.len()
        {
            return Err(InventoryError::Validation(
                "duplicate SKU lines must be combined".to_string(),
            ));
        }
        request.ttl_seconds = Some(
            request
                .ttl_seconds
                .unwrap_or(self.config.reservation_ttl_seconds)
                .clamp(60, 3_600),
        );
        self.repository.reserve(request).await.map_err(repo_error)
    }
    pub(crate) async fn confirm(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::Reservation, InventoryError> {
        self.repository
            .confirm(&request.id)
            .await
            .map_err(repo_error)
    }
    pub(crate) async fn release(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::Reservation, InventoryError> {
        self.repository
            .release(&request.id)
            .await
            .map_err(repo_error)
    }
    pub(crate) async fn expire_reservations(
        &self,
        request: pb::BatchRequest,
    ) -> Result<pb::ExpireReservationsResponse, InventoryError> {
        Ok(pb::ExpireReservationsResponse {
            expired: self
                .repository
                .expire(usize::try_from(request.limit.clamp(1, 1_000)).unwrap_or(1_000))
                .await
                .map_err(repo_error)?,
        })
    }
}
fn repo_error(error: RepositoryError) -> InventoryError {
    match error {
        RepositoryError::NotFound(value) => InventoryError::NotFound(value),
        RepositoryError::Insufficient(value) => InventoryError::Insufficient(value),
        RepositoryError::Conflict(value) => InventoryError::Conflict(value),
        RepositoryError::Failed(value) => InventoryError::Repository(value),
    }
}
