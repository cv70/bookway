use crate::api::pb;
use crate::{
    Config,
    datasource::{CatalogDao, DaoError, MemoryCatalogDao, PostgresCatalogDao},
};
use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
use std::sync::Arc;
use thiserror::Error;
#[derive(Debug, Error)]
pub(crate) enum MallError {
    #[error("{0}")]
    Validation(String),
    #[error("product or SKU {0} was not found")]
    NotFound(String),
    #[error("catalog conflict: {0}")]
    Conflict(String),
    #[error("catalog operation failed: {0}")]
    Repository(String),
}
#[derive(Clone)]
pub struct Domain {
    config: Config,
    dao: Arc<dyn CatalogDao>,
    bbs_link: BbsLinkClient<tonic::transport::Channel>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let dao: Arc<dyn CatalogDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCatalogDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresCatalogDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let bbs_link = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            config,
            dao,
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
        self.dao.create(request).await.map_err(repo_error)
    }
    pub(crate) async fn update_product(
        &self,
        request: pb::UpdateProductRequest,
    ) -> Result<pb::MallProduct, MallError> {
        if request.product_id.trim().is_empty() {
            return Err(MallError::Validation("product id is required".to_string()));
        }
        validate_update(&request)?;
        self.dao.update(request).await.map_err(repo_error)
    }
    pub(crate) async fn products(
        &self,
        request: pb::ProductQueryRequest,
    ) -> Result<pb::ProductPage, MallError> {
        self.dao.list(request).await.map_err(repo_error)
    }
    pub(crate) async fn product(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::MallProduct, MallError> {
        if request.id.trim().is_empty() {
            return Err(MallError::Validation("product id is required".to_string()));
        }
        self.dao.get(&request.id).await.map_err(repo_error)
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
            items: self.dao.skus(request.ids).await.map_err(repo_error)?,
        })
    }

    pub(crate) async fn attach_node_offer(
        &self,
        mut request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if request.product_id.trim().is_empty()
            || request.merchant_id.trim().is_empty()
            || request.sku_id.trim().is_empty()
            || request.route_id.trim().is_empty()
            || request.action_node_id.trim().is_empty()
            || request.scene_equipment.trim().is_empty()
            || request.creator_id.trim().is_empty()
            || request.idempotency_key.trim().is_empty()
        {
            return Err(MallError::Validation(
                "merchant, product, SKU, route, action node, scene equipment, creator and idempotency key are required"
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
            Some(&request.scene_equipment),
        )
        .await?;
        let skus = self
            .dao
            .skus(vec![request.sku_id.clone()])
            .await
            .map_err(repo_error)?;
        if skus.len() != 1 || skus[0].product_id != request.product_id {
            return Err(MallError::NotFound(request.sku_id));
        }
        self.dao
            .attach_node_offer(request)
            .await
            .map_err(repo_error)
    }

    pub(crate) async fn node_offers(
        &self,
        mut request: pb::NodeOfferQueryRequest,
    ) -> Result<pb::NodeOfferList, MallError> {
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if request.route_id.trim().is_empty()
            || request.action_node_id.trim().is_empty()
            || request.scene_equipment.is_empty()
        {
            return Err(MallError::Validation(
                "route, action node and scene equipment are required".to_string(),
            ));
        }
        let route = self.load_public_route(&request.route_id).await?;
        validate_route_action_node(
            &route,
            &request.action_node_id,
            None,
            Some(&request.scene_equipment),
        )?;
        let requested_scene_equipment = request.scene_equipment.clone();
        let action_offers = self.dao.node_offers(request).await.map_err(repo_error)?;
        let items = action_offers
            .into_iter()
            .filter(|offer| {
                validate_route_action_node(
                    &route,
                    &offer.action_node_id,
                    None,
                    Some(&offer.scene_equipment),
                )
                .is_ok()
                    && scene_equipment_key(&offer.scene_equipment) == requested_scene_equipment
            })
            .collect();
        Ok(pb::NodeOfferList { items })
    }

    pub(crate) async fn checkout_node_offer(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        if request.id.trim().is_empty() {
            return Err(MallError::Validation(
                "node offer id is required".to_string(),
            ));
        }
        let offer = self.dao.node_offer(&request.id).await.map_err(repo_error)?;
        // Orders resolve an offer by ID rather than through the public list.
        // Revalidate its route action context so an old offer ID cannot bypass
        // a withdrawn route, a removed action node, or changed equipment.
        self.validate_public_action_node(
            &offer.route_id,
            &offer.action_node_id,
            None,
            Some(&offer.scene_equipment),
        )
        .await?;
        // The route association can remain addressable for historical
        // settlement, but checkout must resolve a currently saleable SKU.
        let product = self.dao.get(&offer.product_id).await.map_err(repo_error)?;
        let skus = self
            .dao
            .skus(vec![offer.sku_id.clone()])
            .await
            .map_err(repo_error)?;
        if !skus
            .iter()
            .any(|sku| sku.id == offer.sku_id && sku.product_id == product.id)
        {
            return Err(MallError::NotFound(offer.sku_id));
        }
        Ok(offer)
    }

    pub(crate) async fn settlement_node_offer(
        &self,
        request: pb::IdRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        if request.id.trim().is_empty() {
            return Err(MallError::Validation(
                "node offer id is required".to_string(),
            ));
        }
        // Historical attribution and affiliate settlement deliberately retain
        // the original context after a merchant withdraws an offer.
        self.dao.node_offer(&request.id).await.map_err(repo_error)
    }

    async fn validate_public_action_node(
        &self,
        route_id: &str,
        action_node_id: &str,
        expected_creator_id: Option<&str>,
        expected_scene_equipment: Option<&str>,
    ) -> Result<(), MallError> {
        let route = self.load_public_route(route_id).await?;
        validate_route_action_node(
            &route,
            action_node_id,
            expected_creator_id,
            expected_scene_equipment,
        )
    }

    async fn load_public_route(&self, route_id: &str) -> Result<bbs_link::Content, MallError> {
        let mut client = self.bbs_link.clone();
        client
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
            })
            .map(|response| response.into_inner())
    }
}

fn validate_route_action_node(
    route: &bbs_link::Content,
    action_node_id: &str,
    expected_creator_id: Option<&str>,
    expected_scene_equipment: Option<&str>,
) -> Result<(), MallError> {
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
    let action = route.route_template.as_ref().and_then(|template| {
        template
            .actions
            .iter()
            .find(|action| action.id == action_node_id)
    });
    let Some(action) = action else {
        return Err(MallError::Validation(
            "action node does not belong to the public route".to_string(),
        ));
    };
    if let Some(scene_equipment) = expected_scene_equipment
        && !action
            .scene_equipment
            .iter()
            .any(|value| scene_equipment_key(value) == scene_equipment_key(scene_equipment))
    {
        return Err(MallError::Validation(
            "scene equipment is not declared by the action node".to_string(),
        ));
    }
    Ok(())
}

fn scene_equipment_key(value: &str) -> String {
    value.trim().to_lowercase()
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
fn repo_error(error: DaoError) -> MallError {
    match error {
        DaoError::NotFound(value) => MallError::NotFound(value),
        DaoError::Conflict(value) => MallError::Conflict(value),
        DaoError::Failed(value) => MallError::Repository(value),
    }
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb as bbs_link;

    use super::{MallError, validate_route_action_node};

    fn public_route() -> bbs_link::Content {
        bbs_link::Content {
            author_id: "route-author".to_string(),
            content_type: bbs_link::ContentType::Route as i32,
            route_template: Some(bbs_link::RouteTemplate {
                actions: vec![bbs_link::RouteTemplateAction {
                    id: "node-1".to_string(),
                    scene_equipment: vec!["trail shoes".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn resolved_offer_must_remain_on_the_current_action_context() {
        let route = public_route();
        validate_route_action_node(&route, "node-1", None, Some("TRAIL SHOES"))
            .expect("current scene equipment remains saleable");
        assert!(matches!(
            validate_route_action_node(&route, "removed-node", None, Some("trail shoes")),
            Err(MallError::Validation(message))
                if message == "action node does not belong to the public route"
        ));
        assert!(matches!(
            validate_route_action_node(&route, "node-1", None, Some("rain shell")),
            Err(MallError::Validation(message))
                if message == "scene equipment is not declared by the action node"
        ));
    }
}
