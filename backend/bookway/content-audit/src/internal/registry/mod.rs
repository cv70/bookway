use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::AuditRepository,
    domain::AuditService,
    service::{self, AppState},
};

pub(crate) async fn build(config: Config) -> Result<Router, bookway_data::DataError> {
    let pool = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Postgres => Some(bookway_data::postgres_pool().await?),
        bookway_data::StorageMode::Memory => None,
    };
    Ok(service::router(AppState {
        audit: AuditService::new(
            Arc::new(AuditRepository::new(pool)),
            config.blocked,
            config.reviewing,
        ),
    }))
}
