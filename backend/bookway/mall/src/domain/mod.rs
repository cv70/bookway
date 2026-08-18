use crate::api::pb;
use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
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
    bbs_link: BbsLinkClient<tonic::transport::Channel>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: Arc<dyn CatalogRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCatalogRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCatalogRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let bbs_link = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            config,
            repository,
            bbs_link,
        })
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

    pub(crate) async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        if request.product_id.trim().is_empty()
            || request.merchant_id.trim().is_empty()
            || request.sku_id.trim().is_empty()
            || request.route_id.trim().is_empty()
            || request.action_node_id.trim().is_empty()
            || request.creator_id.trim().is_empty()
            || request.idempotency_key.trim().is_empty()
        {
            return Err(MallError::Validation(
                "merchant, product, SKU, route, action node, creator and idempotency key are required"
                    .to_string(),
            ));
        }
        if request.commission_bps > 3_000 {
            return Err(MallError::Validation(
                "commission must be between 0 and 3000 basis points".to_string(),
            ));
        }
        self.validate_public_action_node(
            &request.route_id,
            &request.action_node_id,
            Some(&request.creator_id),
        )
        .await?;
        let skus = self
            .repository
            .skus(vec![request.sku_id.clone()])
            .await
            .map_err(repo_error)?;
        if skus.len() != 1 || skus[0].product_id != request.product_id {
            return Err(MallError::NotFound(request.sku_id));
        }
        self.repository
            .attach_node_offer(request)
            .await
            .map_err(repo_error)
    }

    pub(crate) async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<pb::NodeOfferList, MallError> {
        if request.route_id.trim().is_empty() || request.action_node_id.trim().is_empty() {
            return Err(MallError::Validation(
                "route and action node are required".to_string(),
            ));
        }
        self.validate_public_action_node(&request.route_id, &request.action_node_id, None)
            .await?;
        Ok(pb::NodeOfferList {
            items: self
                .repository
                .node_offers(request)
                .await
                .map_err(repo_error)?,
        })
    }

    pub(crate) async fn node_offer(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        if request.id.trim().is_empty() {
            return Err(MallError::Validation(
                "node offer id is required".to_string(),
            ));
        }
        self.repository
            .node_offer(&request.id)
            .await
            .map_err(repo_error)
    }

    async fn validate_public_action_node(
        &self,
        route_id: &str,
        action_node_id: &str,
        expected_creator_id: Option<&str>,
    ) -> Result<(), MallError> {
        let mut client = self.bbs_link.clone();
        let route = client
            .get_public(
                bookway_runtime::grpc_service_request(bbs_link::IdRequest {
                    id: route_id.to_string(),
                })
                .map_err(|error| MallError::Repository(error.to_string()))?,
            )
            .await
            .map_err(|error| match error.code() {
                tonic::Code::NotFound => MallError::NotFound(route_id.to_string()),
                _ => MallError::Repository(format!("bbs-link get_public failed: {error}")),
            })?
            .into_inner();
        if route.content_type != bbs_link::ContentType::Route as i32 {
            return Err(MallError::Validation(
                "equipment can only be attached to a public route action node".to_string(),
            ));
        }
        if expected_creator_id.is_some_and(|creator_id| route.author_id != creator_id) {
            return Err(MallError::Validation(
                "the offer creator must own the attached route".to_string(),
            ));
        }
        let has_node = route
            .route_template
            .as_ref()
            .is_some_and(|template| template.actions.iter().any(|action| action.id == action_node_id));
        if !has_node {
            return Err(MallError::Validation(
                "action node does not belong to the public route".to_string(),
            ));
        }
        Ok(())
    }
}
fn validate(request: &pb::CreateProductRequest) -> Result<(), MallError> {
    if request.merchant_id.trim().is_empty()
        || request.title.trim().is_empty()
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
    if request.merchant_id.trim().is_empty() {
        return Err(MallError::Validation("merchant id is required".to_string()));
    }
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
