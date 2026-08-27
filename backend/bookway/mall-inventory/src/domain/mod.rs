use crate::api::pb;
use crate::{
    Config,
    datasource::{
        DaoError, InventoryDao, MemoryInventoryDao, PostgresInventoryDao, RedisInventoryDao,
    },
};
use std::sync::Arc;
use thiserror::Error;
const MAX_IDENTIFIER_LENGTH: usize = 160;
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
    dao: Arc<dyn InventoryDao>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let dao: Arc<dyn InventoryDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryInventoryDao::default()),
            bookway_data::StorageMode::Postgres => {
                let postgres = PostgresInventoryDao::new(bookway_data::postgres_pool().await?);
                match bookway_data::redis_connection().await {
                    Ok(Some(redis)) => Arc::new(RedisInventoryDao::new(postgres, redis)),
                    Ok(None) => Arc::new(postgres),
                    Err(error) => {
                        tracing::warn!(%error, "redis unavailable at startup; inventory uses PostgreSQL reservations");
                        Arc::new(postgres)
                    }
                }
            }
        };
        Ok(Self { config, dao })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn set_stock(
        &self,
        mut request: pb::SetStockRequest,
    ) -> Result<pb::InventoryItem, InventoryError> {
        request.sku_id = request.sku_id.trim().to_string();
        if invalid_identifier(&request.sku_id) || request.available < 0 {
            return Err(InventoryError::Validation(
                "sku_id and a non-negative available count are required".to_string(),
            ));
        }
        self.dao.set_stock(request).await.map_err(repo_error)
    }
    pub(crate) async fn stock(
        &self,
        mut request: pb::IdRequest,
    ) -> Result<pb::InventoryItem, InventoryError> {
        request.id = request.id.trim().to_string();
        if invalid_identifier(&request.id) {
            return Err(InventoryError::Validation("sku id is required".to_string()));
        }
        self.dao.stock(&request.id).await.map_err(repo_error)
    }
    pub(crate) async fn reserve(
        &self,
        mut request: pb::ReserveRequest,
    ) -> Result<pb::Reservation, InventoryError> {
        request.reservation_id = request.reservation_id.trim().to_string();
        for item in &mut request.items {
            item.sku_id = item.sku_id.trim().to_string();
        }
        if invalid_identifier(&request.reservation_id)
            || request.items.is_empty()
            || request.items.len() > 100
            || request
                .items
                .iter()
                .any(|item| invalid_identifier(&item.sku_id) || item.quantity == 0)
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
        self.dao.reserve(request).await.map_err(repo_error)
    }
    pub(crate) async fn confirm(
        &self,
        mut request: pb::IdRequest,
    ) -> Result<pb::Reservation, InventoryError> {
        request.id = request.id.trim().to_string();
        if invalid_identifier(&request.id) {
            return Err(InventoryError::Validation(
                "reservation id is required".to_string(),
            ));
        }
        self.dao.confirm(&request.id).await.map_err(repo_error)
    }
    pub(crate) async fn release(
        &self,
        mut request: pb::IdRequest,
    ) -> Result<pb::Reservation, InventoryError> {
        request.id = request.id.trim().to_string();
        if invalid_identifier(&request.id) {
            return Err(InventoryError::Validation(
                "reservation id is required".to_string(),
            ));
        }
        self.dao.release(&request.id).await.map_err(repo_error)
    }
    pub(crate) async fn expire_reservations(
        &self,
        request: pb::BatchRequest,
    ) -> Result<pb::ExpireReservationsResponse, InventoryError> {
        Ok(pb::ExpireReservationsResponse {
            expired: self
                .dao
                .expire(usize::try_from(request.limit.clamp(1, 1_000)).unwrap_or(1_000))
                .await
                .map_err(repo_error)?,
        })
    }
}
fn invalid_identifier(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.chars().count() > MAX_IDENTIFIER_LENGTH
}
fn repo_error(error: DaoError) -> InventoryError {
    match error {
        DaoError::NotFound(value) => InventoryError::NotFound(value),
        DaoError::Insufficient(value) => InventoryError::Insufficient(value),
        DaoError::Conflict(value) => InventoryError::Conflict(value),
        DaoError::Failed(value) => InventoryError::Repository(value),
    }
}
