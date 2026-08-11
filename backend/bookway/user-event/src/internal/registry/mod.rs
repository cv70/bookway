use std::sync::Arc;

use axum::Router;

use super::{
    datasource::{EventRepository, MemoryEventRepository, PostgresEventRepository},
    domain::UserEventService,
    service::{self, AppState},
};

pub(crate) async fn build() -> Result<Router, bookway_data::DataError> {
    let repository: Arc<dyn EventRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryEventRepository::default()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresEventRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    Ok(service::router(AppState {
        events: UserEventService::new(repository),
    }))
}
