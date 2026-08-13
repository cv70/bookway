use std::sync::Arc;

use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        MemoryMediaRepository, ObjectError, ObjectStorage, PostgresMediaRepository,
        RepositoryError, SharedMediaRepository,
    },
};

#[derive(Debug, Error)]
pub(crate) enum MediaError {
    #[error("{0}")]
    Validation(String),
    #[error("proxy upload is disabled")]
    Forbidden,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) repository: SharedMediaRepository,
    pub(crate) objects: Arc<ObjectStorage>,
    pub(crate) bucket: String,
    pub(crate) cdn_base: String,
    pub(crate) proxy_upload: bool,
}
impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: SharedMediaRepository = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryMediaRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresMediaRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let objects = Arc::new(ObjectStorage::new(
            &config.s3_endpoint,
            config.s3_bucket.clone(),
            config.s3_region.clone(),
            config.s3_access_key.clone(),
            config.s3_secret_key.clone(),
        )?);
        Ok(Self {
            config: config.clone(),
            repository,
            objects,
            bucket: config.s3_bucket,
            cdn_base: config.cdn_base,
            proxy_upload: config.proxy_upload,
        })
    }
}
