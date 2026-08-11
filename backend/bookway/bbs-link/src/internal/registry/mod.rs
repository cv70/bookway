use std::sync::Arc;

use axum::Router;

use super::{
    datasource::{
        ContentAuditor, ContentRepository, HttpContentAuditor, LocalContentAuditor,
        MemoryContentRepository, PostgresContentRepository,
    },
    domain::ContentService,
    service::{self, AppState},
};

pub(crate) async fn build() -> Result<Router, bookway_data::DataError> {
    let repository: Arc<dyn ContentRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryContentRepository::seeded()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresContentRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    let auditor: Arc<dyn ContentAuditor> = match std::env::var("CONTENT_AUDIT_URL") {
        Ok(url) if !url.trim().is_empty() => Arc::new(HttpContentAuditor::new(url)),
        _ => Arc::new(LocalContentAuditor),
    };
    Ok(service::router(AppState {
        content: ContentService::new(repository).with_auditor(auditor),
    }))
}
