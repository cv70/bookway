use std::sync::Arc;

use axum::Router;

use super::{
    datasource::{BbsRepository, MemoryBbsRepository, PostgresBbsRepository},
    domain::BbsService,
    service::{self, AppState},
};

pub(crate) async fn build() -> Result<Router, bookway_data::DataError> {
    let repository: Arc<dyn BbsRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryBbsRepository::seeded()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresBbsRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    Ok(service::router(AppState {
        bbs: BbsService::new(repository),
    }))
}
