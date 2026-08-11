use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::{MediaRepository, MemoryMediaRepository, ObjectStorage, PostgresMediaRepository},
    domain::MediaService,
    service::{self, AppState},
};

pub(crate) async fn build(config: Config) -> Result<Router, Box<dyn std::error::Error>> {
    let repository: Arc<dyn MediaRepository> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryMediaRepository::default()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresMediaRepository::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    let objects = Arc::new(ObjectStorage::new(
        &config.s3_endpoint,
        config.s3_bucket.clone(),
        config.s3_region,
        config.s3_access_key,
        config.s3_secret_key,
    )?);
    Ok(service::router(AppState {
        media: MediaService::new(
            repository,
            objects,
            config.s3_bucket,
            config.cdn_base,
            config.proxy_upload,
        ),
    }))
}
