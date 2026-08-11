use std::sync::Arc;

use axum::Router;

use super::{
    datasource::{GrowthRepository, MemoryGrowthRepository, PostgresGrowthRepository},
    domain::GrowthService,
    service::{self, AppState},
};

pub(crate) async fn build() -> Result<Router, bookway_data::DataError> {
    let repository: Arc<dyn GrowthRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryGrowthRepository::seeded()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresGrowthRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    let growth = GrowthService::new(repository);
    Ok(service::router(AppState { growth }))
}
