use std::sync::Arc;

use axum::Router;

use super::{
    datasource::{CommentRepository, MemoryCommentRepository, PostgresCommentRepository},
    domain::CommentService,
    service::{self, AppState},
};

pub(crate) async fn build() -> Result<Router, bookway_data::DataError> {
    let repository: Arc<dyn CommentRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryCommentRepository::default()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresCommentRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    Ok(service::router(AppState {
        comment: CommentService::new(repository),
    }))
}
