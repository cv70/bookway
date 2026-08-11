use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::{FeatureCache, FeatureRepository},
    domain::FeatureService,
    service::{self, AppState},
};

pub(crate) async fn build(config: Config) -> Result<Router, bookway_data::DataError> {
    let pool = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Postgres => Some(bookway_data::postgres_pool().await?),
        bookway_data::StorageMode::Memory => None,
    };
    let redis = match bookway_data::redis_connection().await {
        Ok(redis) => redis,
        Err(error) => {
            tracing::warn!(%error, "redis unavailable at startup; feature cache disabled");
            None
        }
    };
    let service = FeatureService::new(
        Arc::new(FeatureRepository::new(pool)),
        Arc::new(FeatureCache::new(redis)),
        config.model_version,
    );
    Ok(service::router(AppState { features: service }))
}
