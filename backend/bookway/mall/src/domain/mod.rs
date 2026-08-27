use crate::api::pb;
use crate::{
    Config,
    datasource::{CachedCatalogDao, CatalogDao, DaoError, MemoryCatalogDao, PostgresCatalogDao},
};
use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
use bookway_knowledge_catalog_api::pb::{
    self as catalog, knowledge_catalog_client::KnowledgeCatalogClient,
};
use std::sync::Arc;
use thiserror::Error;

const MAX_IDENTIFIER_LENGTH: usize = 160;
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 220;
const MAX_PRODUCT_TEXT_LENGTH: usize = 4_000;
const MAX_SKU_TITLE_LENGTH: usize = 200;
const MAX_CURRENCY_LENGTH: usize = 16;
const MAX_QUERY_LENGTH: usize = 100;

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
    knowledge_catalog: KnowledgeCatalogClient<tonic::transport::Channel>,
}
impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let dao: Arc<dyn CatalogDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryCatalogDao::default()),
            bookway_data::StorageMode::Postgres => {
                // Public catalog reads are hot (route nodes, checkout prep);
                // wrap the Postgres dao with the versioned read-through cache.
                // No REDIS_URL keeps behavior identical to an uncached dao.
                let postgres = Arc::new(PostgresCatalogDao::new(
                    bookway_data::postgres_pool().await?,
                ));
                let redis = match bookway_data::redis_connection().await {
                    Ok(redis) => redis,
                    Err(error) => {
                        tracing::warn!(%error, "redis unavailable at startup; catalog cache disabled");
                        None
                    }
                };
                Arc::new(CachedCatalogDao::new(postgres, redis))
            }
        };
        let bbs_link =
            BbsLinkClient::new(bookway_runtime::grpc_channel(&config.bbs_link_url).await?);
        // Knowledge-product writes validate against knowledge-catalog. The URL
        // syntax is checked here, but the connection is lazy so the mall still
        // boots while that service is down — binding validation then fails
        // closed at write time instead of startup.
        let endpoint = tonic::transport::Endpoint::from_shared(
            config.knowledge_catalog_url.clone(),
        )?;
        let knowledge_catalog = KnowledgeCatalogClient::new(endpoint.connect_lazy());
        Ok(Self {
            config,
            dao,
            bbs_link,
            knowledge_catalog,
        })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn create_product(
        &self,
        mut request: pb::CreateProductRequest,
    ) -> Result<pb::MallProduct, MallError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.title = request.title.trim().to_string();
        request.description = request.description.trim().to_string();
        request.image_url = request.image_url.trim().to_string();
        for sku in &mut request.skus {
            sku.title = sku.title.trim().to_string();
            sku.currency = sku.currency.trim().to_string();
        }
        validate(&request)?;
        let (kind, resource_id) =
            normalize_binding(request.product_kind, request.course_resource_id)?;
        self.ensure_knowledge_resource(kind, &resource_id).await?;
        request.product_kind = kind as i32;
        request.course_resource_id = resource_id;
        self.dao.create(request).await.map_err(repo_error)
    }
    pub(crate) async fn update_product(
        &self,
        mut request: pb::UpdateProductRequest,
    ) -> Result<pb::MallProduct, MallError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.product_id = request.product_id.trim().to_string();
        if invalid_identifier(&request.product_id) {
            return Err(MallError::Validation("product id is required".to_string()));
        }
        request.title = request.title.map(|value| value.trim().to_string());
        request.description = request.description.map(|value| value.trim().to_string());
        request.image_url = request.image_url.map(|value| value.trim().to_string());
        for sku in &mut request.sku_updates {
            sku.sku_id = sku.sku_id.trim().to_string();
            sku.title = sku.title.take().map(|value| value.trim().to_string());
            sku.currency = sku.currency.take().map(|value| value.trim().to_string());
        }
        if request
            .status
            .is_some_and(|status| pb::MallProductStatus::try_from(status).is_err())
        {
            return Err(MallError::Validation("invalid product status".to_string()));
        }
        // A partially supplied binding would be merged from stale stored state
        // under COALESCE; require merchants to state the complete pair so the
        // validation below always judges the exact values about to persist.
        if request.product_kind.is_some() != request.course_resource_id.is_some() {
            return Err(MallError::Validation(
                "changing the catalogue binding requires both product_kind and course_resource_id"
                    .to_string(),
            ));
        }
        if let Some(kind_value) = request.product_kind {
            let (kind, resource_id) =
                normalize_binding(kind_value, request.course_resource_id.clone().unwrap_or_default())?;
            self.ensure_knowledge_resource(kind, &resource_id).await?;
        }
        validate_update(&request)?;
        self.dao.update(request).await.map_err(repo_error)
    }
    pub(crate) async fn products(
        &self,
        mut request: pb::ProductQueryRequest,
    ) -> Result<pb::ProductPage, MallError> {
        trim_optional(&mut request.merchant_id);
        trim_optional(&mut request.cursor);
        trim_optional(&mut request.query);
        if request
            .merchant_id
            .as_deref()
            .is_some_and(invalid_identifier)
            || request
                .cursor
                .as_deref()
                .is_some_and(|value| value.chars().count() > MAX_IDENTIFIER_LENGTH)
            || request
                .query
                .as_deref()
                .is_some_and(|value| value.chars().count() > MAX_QUERY_LENGTH)
        {
            return Err(MallError::Validation(
                "merchant, cursor and query fields are too long or invalid".to_string(),
            ));
        }
        self.dao.list(request).await.map_err(repo_error)
    }
    pub(crate) async fn product(
        &self,
        mut request: pb::IdRequest,
    ) -> Result<pb::MallProduct, MallError> {
        request.id = request.id.trim().to_string();
        if invalid_identifier(&request.id) {
            return Err(MallError::Validation("product id is required".to_string()));
        }
        self.dao.get(&request.id).await.map_err(repo_error)
    }
    pub(crate) async fn skus(
        &self,
        mut request: pb::SkuIdsRequest,
    ) -> Result<pb::SkuListResponse, MallError> {
        request.ids = request
            .ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .collect();
        if request.ids.is_empty() || request.ids.len() > 100 {
            return Err(MallError::Validation(
                "between 1 and 100 sku ids are required".to_string(),
            ));
        }
        if request.ids.iter().any(|id| invalid_identifier(id)) {
            return Err(MallError::Validation(
                "SKU ids must be non-empty and bounded".to_string(),
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
        request.merchant_id = request.merchant_id.trim().to_string();
        request.product_id = request.product_id.trim().to_string();
        request.sku_id = request.sku_id.trim().to_string();
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        request.creator_id = request.creator_id.trim().to_string();
        request.idempotency_key = request.idempotency_key.trim().to_string();
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if invalid_identifier(&request.product_id)
            || invalid_identifier(&request.merchant_id)
            || invalid_identifier(&request.sku_id)
            || invalid_identifier(&request.route_id)
            || invalid_identifier(&request.action_node_id)
            || invalid_identifier(&request.scene_equipment)
            || invalid_identifier(&request.creator_id)
            || request.idempotency_key.is_empty()
            || request.idempotency_key.chars().count() > MAX_IDEMPOTENCY_KEY_LENGTH
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
        // Course and resource-pack products travel the identical chain: their
        // catalogue binding was already validated at create/update time, so
        // attaching an offer only needs ownership of a currently saleable SKU.
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
        request.route_id = request.route_id.trim().to_string();
        request.action_node_id = request.action_node_id.trim().to_string();
        request.scene_equipment = scene_equipment_key(&request.scene_equipment);
        if invalid_identifier(&request.route_id)
            || invalid_identifier(&request.action_node_id)
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
        mut request: pb::IdRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        request.id = request.id.trim().to_string();
        if invalid_identifier(&request.id) {
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
        mut request: pb::IdRequest,
    ) -> Result<pb::NodeOffer, MallError> {
        request.id = request.id.trim().to_string();
        if invalid_identifier(&request.id) {
            return Err(MallError::Validation(
                "node offer id is required".to_string(),
            ));
        }
        // Historical attribution and affiliate settlement deliberately retain
        // the original context after a merchant withdraws an offer.
        self.dao.node_offer(&request.id).await.map_err(repo_error)
    }

    pub(crate) async fn verify_merchant_sku(
        &self,
        mut request: pb::MerchantSkuRequest,
    ) -> Result<pb::MerchantSkuDecision, MallError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.sku_id = request.sku_id.trim().to_string();
        if invalid_identifier(&request.merchant_id) || invalid_identifier(&request.sku_id) {
            return Err(MallError::Validation(
                "merchant id and sku id are required".to_string(),
            ));
        }
        self.dao
            .verify_merchant_sku(request)
            .await
            .map_err(repo_error)
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

    /// Write-time cross-service validation for catalogue bindings. Physical
    /// goods never touch knowledge-catalog; knowledge products must resolve a
    /// currently published public resource, and any lookup failure rejects the
    /// write (fail closed) so unbound courses can never reach checkout.
    async fn ensure_knowledge_resource(
        &self,
        kind: pb::MallProductKind,
        resource_id: &str,
    ) -> Result<(), MallError> {
        if kind == pb::MallProductKind::Physical {
            return Ok(());
        }
        let request = bookway_runtime::grpc_service_request(catalog::GetRequest {
            resource_id: resource_id.to_string(),
        })
        .map_err(|error| MallError::Repository(error.to_string()))?;
        let looked_up = match self.knowledge_catalog.clone().get(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(status) => match status.code() {
                tonic::Code::NotFound => {
                    return Err(MallError::Validation(
                        "bound knowledge-catalog resource does not exist or is not publicly visible"
                            .to_string(),
                    ));
                }
                _ => Err(format!("knowledge-catalog lookup failed: {status}")),
            },
        };
        judge_catalog_resource(kind, looked_up)
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

/// Validate the (kind, resource id) pair in isolation. The lookup itself is
/// performed by [`Domain::ensure_knowledge_resource`]; this decides what
/// binding shape each catalogue kind requires.
fn normalize_binding(
    kind_value: i32,
    course_resource_id: String,
) -> Result<(pb::MallProductKind, String), MallError> {
    let kind = pb::MallProductKind::try_from(kind_value)
        .ok()
        .ok_or_else(|| MallError::Validation("invalid product kind".to_string()))?;
    let resource_id = course_resource_id.trim().to_string();
    if invalid_identifier(&resource_id) && !resource_id.is_empty() {
        return Err(MallError::Validation(
            "knowledge resource id is too long".to_string(),
        ));
    }
    match kind {
        pb::MallProductKind::Physical if !resource_id.is_empty() => Err(MallError::Validation(
            "physical products cannot bind a knowledge-catalog resource".to_string(),
        )),
        pb::MallProductKind::Physical => Ok((kind, resource_id)),
        _ if resource_id.is_empty() => Err(MallError::Validation(
            "knowledge products must reference a published knowledge-catalog resource"
                .to_string(),
        )),
        _ => Ok((kind, resource_id)),
    }
}

/// Judge a knowledge-catalog Get outcome against the requested product kind.
/// `Err` models an unreachable or failed lookup and maps to Repository so
/// knowledge-product writes fail closed when the catalog cannot confirm the
/// binding.
fn judge_catalog_resource(
    kind: pb::MallProductKind,
    looked_up: Result<catalog::Resource, String>,
) -> Result<(), MallError> {
    let resource = looked_up.map_err(MallError::Repository)?;
    if resource.status != catalog::ResourceStatus::Published as i32 {
        return Err(MallError::Validation(
            "bound knowledge-catalog resource is not publicly published".to_string(),
        ));
    }
    if kind == pb::MallProductKind::Course
        && resource.kind != catalog::ResourceKind::Course as i32
    {
        return Err(MallError::Validation(
            "course products must bind a knowledge-catalog course resource".to_string(),
        ));
    }
    Ok(())
}

fn invalid_identifier(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.chars().count() > MAX_IDENTIFIER_LENGTH
}

fn trim_optional(value: &mut Option<String>) {
    if let Some(text) = value.as_mut() {
        *text = text.trim().to_string();
        if text.is_empty() {
            *value = None;
        }
    }
}

fn validate(request: &pb::CreateProductRequest) -> Result<(), MallError> {
    if pb::MallProductStatus::try_from(request.status).is_err() {
        return Err(MallError::Validation("invalid product status".to_string()));
    }
    if invalid_identifier(&request.merchant_id)
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
        sku.title.trim().is_empty()
            || sku.title.chars().count() > MAX_SKU_TITLE_LENGTH
            || sku.price_cents < 0
            || sku.currency.trim().is_empty()
            || sku.currency.chars().count() > MAX_CURRENCY_LENGTH
    }) {
        return Err(MallError::Validation(
            "each SKU needs a title, currency and non-negative price".to_string(),
        ));
    }
    if request.description.chars().count() > MAX_PRODUCT_TEXT_LENGTH
        || request.image_url.chars().count() > MAX_PRODUCT_TEXT_LENGTH
    {
        return Err(MallError::Validation(
            "product description and image URL are too long".to_string(),
        ));
    }
    Ok(())
}
fn validate_update(request: &pb::UpdateProductRequest) -> Result<(), MallError> {
    if invalid_identifier(&request.merchant_id) {
        return Err(MallError::Validation("merchant id is required".to_string()));
    }
    if request.title.is_none()
        && request.description.is_none()
        && request.image_url.is_none()
        && request.status.is_none()
        && request.product_kind.is_none()
        && request.course_resource_id.is_none()
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
    if request
        .description
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_PRODUCT_TEXT_LENGTH)
        || request
            .image_url
            .as_ref()
            .is_some_and(|value| value.chars().count() > MAX_PRODUCT_TEXT_LENGTH)
    {
        return Err(MallError::Validation(
            "product description and image URL are too long".to_string(),
        ));
    }
    if request.sku_updates.len() > 100
        || request.sku_updates.iter().any(|sku| {
            invalid_identifier(&sku.sku_id)
                || sku
                    .title
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty())
                || sku
                    .title
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > MAX_SKU_TITLE_LENGTH)
                || sku.price_cents.is_some_and(|value| value < 0)
                || sku
                    .currency
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty())
                || sku
                    .currency
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > MAX_CURRENCY_LENGTH)
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
    use crate::api::pb;
    use bookway_bbs_link_api::pb as bbs_link;
    use bookway_knowledge_catalog_api::pb as catalog;

    use super::{
        MallError, judge_catalog_resource, normalize_binding, trim_optional,
        validate_route_action_node,
    };

    #[test]
    fn optional_product_filters_are_trimmed_and_blank_values_are_unset() {
        let mut value = Some("  merchant-a  ".to_string());
        trim_optional(&mut value);
        assert_eq!(value.as_deref(), Some("merchant-a"));

        let mut blank = Some("   ".to_string());
        trim_optional(&mut blank);
        assert!(blank.is_none());
    }

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

    fn published_course() -> catalog::Resource {
        catalog::Resource {
            kind: catalog::ResourceKind::Course as i32,
            status: catalog::ResourceStatus::Published as i32,
            ..Default::default()
        }
    }

    #[test]
    fn catalogue_bindings_require_a_complete_and_kind_appropriate_pair() {
        let (kind, resource_id) =
            normalize_binding(pb::MallProductKind::Physical as i32, String::new())
                .expect("physical goods need no knowledge binding");
        assert_eq!(kind, pb::MallProductKind::Physical);
        assert!(resource_id.is_empty());

        let (kind, resource_id) = normalize_binding(
            pb::MallProductKind::Course as i32,
            " resource-1 ".to_string(),
        )
        .expect("course bindings accept a public resource id");
        assert_eq!(kind, pb::MallProductKind::Course);
        assert_eq!(resource_id, "resource-1");

        assert!(matches!(
            normalize_binding(99, String::new()),
            Err(MallError::Validation(message)) if message == "invalid product kind"
        ));
        assert!(matches!(
            normalize_binding(
                pb::MallProductKind::Physical as i32,
                "resource-1".to_string()
            ),
            Err(MallError::Validation(message))
                if message == "physical products cannot bind a knowledge-catalog resource"
        ));
        assert!(matches!(
            normalize_binding(pb::MallProductKind::Course as i32, "  ".to_string()),
            Err(MallError::Validation(message))
                if message == "knowledge products must reference a published knowledge-catalog resource"
        ));
    }

    #[test]
    fn course_products_only_bind_published_course_resources() {
        judge_catalog_resource(
            pb::MallProductKind::Course,
            Ok(published_course()),
        )
        .expect("a published course resource is a legal course binding");

        judge_catalog_resource(
            pb::MallProductKind::ResourcePack,
            Ok(published_course()),
        )
        .expect("any published resource backs a resource pack");

        let mut book = published_course();
        book.kind = catalog::ResourceKind::Book as i32;
        assert!(matches!(
            judge_catalog_resource(pb::MallProductKind::Course, Ok(book)),
            Err(MallError::Validation(message))
                if message == "course products must bind a knowledge-catalog course resource"
        ));

        let mut archived = published_course();
        archived.status = catalog::ResourceStatus::Archived as i32;
        assert!(matches!(
            judge_catalog_resource(pb::MallProductKind::ResourcePack, Ok(archived)),
            Err(MallError::Validation(message))
                if message == "bound knowledge-catalog resource is not publicly published"
        ));
    }

    #[test]
    fn knowledge_writes_fail_closed_when_the_catalog_cannot_confirm_bindings() {
        assert!(matches!(
            judge_catalog_resource(
                pb::MallProductKind::Course,
                Err("knowledge-catalog lookup failed: transport error".to_string()),
            ),
            Err(MallError::Repository(message))
                if message.contains("transport error")
        ));
    }
}
