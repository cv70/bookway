use crate::api::pb;
use crate::{
    Config,
    datasource::{
        CatalogRepository, MemoryCatalogRepository, PostgresCatalogRepository, RepositoryError,
    },
};
use std::sync::Arc;
use thiserror::Error;
#[derive(Debug, Error)]
pub(crate) enum MallError {
    #[error("{0}")]
    Validation(String),
    #[error("product or SKU {0} was not found")]
    NotFound(String),
    #[error("catalog operation failed: {0}")]
    Repository(String),
}
#[derive(Clone)]
pub struct Domain {
    config: Config,
    repository: Arc<dyn CatalogRepository>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: Arc<dyn CatalogRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCatalogRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCatalogRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self { config, repository })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn create_product(
        &self,
        request: pb::CreateProductRequest,
    ) -> Result<pb::MallProduct, MallError> {
        validate(&request)?;
        self.repository.create(request).await.map_err(repo_error)
    }
    pub(crate) async fn update_product(
        &self,
        request: pb::UpdateProductRequest,
    ) -> Result<pb::MallProduct, MallError> {
        if request.product_id.trim().is_empty() {
            return Err(MallError::Validation("product id is required".to_string()));
        }
        validate_update(&request)?;
        self.repository.update(request).await.map_err(repo_error)
    }
    pub(crate) async fn products(
        &self,
        request: pb::ProductQueryRequest,
    ) -> Result<pb::ProductPage, MallError> {
        self.repository.list(request).await.map_err(repo_error)
    }
    pub(crate) async fn product(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::MallProduct, MallError> {
        if request.id.trim().is_empty() {
            return Err(MallError::Validation("product id is required".to_string()));
        }
        self.repository.get(&request.id).await.map_err(repo_error)
    }
    pub(crate) async fn skus(
        &self,
        request: pb::SkuIdsRequest,
    ) -> Result<pb::SkuListResponse, MallError> {
        if request.ids.is_empty() || request.ids.len() > 100 {
            return Err(MallError::Validation(
                "between 1 and 100 sku ids are required".to_string(),
            ));
        }
        Ok(pb::SkuListResponse {
            items: self
                .repository
                .skus(request.ids)
                .await
                .map_err(repo_error)?,
        })
    }
}
fn validate(request: &pb::CreateProductRequest) -> Result<(), MallError> {
    if request.title.trim().is_empty()
        || request.title.chars().count() > 120
        || request.skus.is_empty()
        || request.skus.len() > 100
    {
        return Err(MallError::Validation(
            "a title and 1-100 SKUs are required".to_string(),
        ));
    }
    if request.skus.iter().any(|sku| {
        sku.title.trim().is_empty() || sku.price_cents < 0 || sku.currency.trim().is_empty()
    }) {
        return Err(MallError::Validation(
            "each SKU needs a title, currency and non-negative price".to_string(),
        ));
    }
    Ok(())
}
fn validate_update(request: &pb::UpdateProductRequest) -> Result<(), MallError> {
    if request.title.is_none()
        && request.description.is_none()
        && request.image_url.is_none()
        && request.status.is_none()
        && request.sku_updates.is_empty()
    {
        return Err(MallError::Validation(
            "at least one product or SKU field is required".to_string(),
        ));
    }
    if request
        .title
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.chars().count() > 120)
    {
        return Err(MallError::Validation(
            "title must contain at most 120 characters".to_string(),
        ));
    }
    if request.sku_updates.len() > 100
        || request.sku_updates.iter().any(|sku| {
            sku.sku_id.trim().is_empty()
                || sku
                    .title
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty())
                || sku.price_cents.is_some_and(|value| value < 0)
                || sku
                    .currency
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty())
                || (sku.title.is_none()
                    && sku.price_cents.is_none()
                    && sku.currency.is_none()
                    && sku.attributes.is_none()
                    && sku.saleable.is_none())
        })
        || request
            .sku_updates
            .iter()
            .map(|sku| &sku.sku_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != request.sku_updates.len()
    {
        return Err(MallError::Validation(
            "SKU updates need unique ids and at least one valid field".to_string(),
        ));
    }
    Ok(())
}
fn repo_error(error: RepositoryError) -> MallError {
    match error {
        RepositoryError::NotFound(value) => MallError::NotFound(value),
        RepositoryError::Failed(value) => MallError::Repository(value),
    }
}
