use std::sync::Arc;

use axum::Router;

use super::{
    datasource::{LikeStatusRepository, MemoryLikeStatusRepository, PostgresLikeStatusRepository},
    domain::LikeStatusService,
    service::{self, AppState},
};

pub(crate) async fn build() -> Result<Router, bookway_data::DataError> {
    let repository: Arc<dyn LikeStatusRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryLikeStatusRepository::seeded()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresLikeStatusRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    Ok(service::router(AppState {
        like_status: LikeStatusService::new(repository),
    }))
}
