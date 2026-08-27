use crate::api::pb;
use async_trait::async_trait;
use std::collections::HashMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) enum DaoError {
    NotFound(String),
    Conflict(String),
    Failed(String),
}
#[async_trait]
pub(crate) trait CatalogDao: Send + Sync {
    async fn create(&self, request: pb::CreateProductRequest) -> Result<pb::MallProduct, DaoError>;
    async fn update(&self, request: pb::UpdateProductRequest) -> Result<pb::MallProduct, DaoError>;
    async fn list(&self, request: pb::ProductQueryRequest) -> Result<pb::ProductPage, DaoError>;
    async fn get(&self, id: &str) -> Result<pb::MallProduct, DaoError>;
    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, DaoError>;
    async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, DaoError>;
    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, DaoError>;
    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, DaoError>;
    /// Whether the merchant owns the product carrying this SKU. Ownership is
    /// judged live (no cache indirection) because it authorizes a mutation.
    async fn verify_merchant_sku(
        &self,
        request: pb::MerchantSkuRequest,
    ) -> Result<pb::MerchantSkuDecision, DaoError>;
}

fn new_product(request: pb::CreateProductRequest) -> pb::MallProduct {
    let now = timestamp(OffsetDateTime::now_utc());
    let product_id = Uuid::now_v7().to_string();
    let skus = request
        .skus
        .into_iter()
        .map(|sku| pb::MallSku {
            id: Uuid::now_v7().to_string(),
            product_id: product_id.clone(),
            title: sku.title,
            price_cents: sku.price_cents,
            currency: sku.currency,
            attributes: sku.attributes,
            saleable: sku.saleable,
        })
        .collect();
    pb::MallProduct {
        id: product_id,
        title: request.title,
        description: request.description,
        image_url: request.image_url,
        status: request.status,
        skus,
        product_kind: request.product_kind,
        course_resource_id: request.course_resource_id,
        created_at: now.clone(),
        updated_at: now,
    }
}
fn apply_update(
    product: &mut pb::MallProduct,
    request: pb::UpdateProductRequest,
) -> Result<(), DaoError> {
    if let Some(value) = request.title {
        product.title = value;
    }
    if let Some(value) = request.description {
        product.description = value;
    }
    if let Some(value) = request.image_url {
        product.image_url = value;
    }
    if let Some(value) = request.status {
        product.status = value;
    }
    // The domain layer only forwards a complete (kind, resource id) pair, so
    // an accepted update never desynchronises the catalogue binding halves.
    if let Some(kind) = request.product_kind {
        product.product_kind = kind;
    }
    if let Some(resource_id) = request.course_resource_id {
        product.course_resource_id = resource_id;
    }
    for update in request.sku_updates {
        let sku = product
            .skus
            .iter_mut()
            .find(|sku| sku.id == update.sku_id)
            .ok_or_else(|| DaoError::NotFound(update.sku_id.clone()))?;
        if let Some(value) = update.title {
            sku.title = value;
        }
        if let Some(value) = update.price_cents {
            sku.price_cents = value;
        }
        if let Some(value) = update.currency {
            sku.currency = value;
        }
        if let Some(value) = update.attributes {
            sku.attributes = value.values;
        }
        if let Some(value) = update.saleable {
            sku.saleable = value;
        }
    }
    product.updated_at = timestamp(OffsetDateTime::now_utc());
    Ok(())
}
fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
fn status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::MallProductStatus::try_from(value).ok() {
        Some(pb::MallProductStatus::Draft) => Ok("draft"),
        Some(pb::MallProductStatus::Active) => Ok("active"),
        Some(pb::MallProductStatus::Archived) => Ok("archived"),
        None => Err(DaoError::Failed("invalid product status".to_string())),
    }
}
fn parse_status(value: &str) -> Result<i32, DaoError> {
    match value {
        "draft" => Ok(pb::MallProductStatus::Draft as i32),
        "active" => Ok(pb::MallProductStatus::Active as i32),
        "archived" => Ok(pb::MallProductStatus::Archived as i32),
        _ => Err(DaoError::Failed(format!("unknown product status {value}"))),
    }
}
/// Catalogue kinds persist as their lowercase proto value names, matching the
/// CHECK constraint installed by migration 0076.
fn kind_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::MallProductKind::try_from(value).ok() {
        Some(pb::MallProductKind::Physical) => Ok("physical"),
        Some(pb::MallProductKind::Course) => Ok("course"),
        Some(pb::MallProductKind::ResourcePack) => Ok("resource_pack"),
        None => Err(DaoError::Failed("invalid product kind".to_string())),
    }
}
fn parse_kind(value: &str) -> Result<i32, DaoError> {
    match value {
        "physical" => Ok(pb::MallProductKind::Physical as i32),
        "course" => Ok(pb::MallProductKind::Course as i32),
        "resource_pack" => Ok(pb::MallProductKind::ResourcePack as i32),
        _ => Err(DaoError::Failed(format!("unknown product kind {value}"))),
    }
}
fn database(error: sqlx::Error) -> DaoError {
    DaoError::Failed(error.to_string())
}

fn offer_matches_request(offer: &pb::NodeOffer, request: &pb::AttachNodeOfferRequest) -> bool {
    offer.merchant_id == request.merchant_id
        && offer.product_id == request.product_id
        && offer.sku_id == request.sku_id
        && offer.route_id == request.route_id
        && offer.action_node_id == request.action_node_id
        && offer.scene_equipment == request.scene_equipment
        && offer.creator_id == request.creator_id
        && offer.commission_bps == request.commission_bps
}

#[cfg(test)]
mod tests {
    use super::{CatalogDao, DaoError, MemoryCatalogDao};
    use crate::api::pb;

    #[tokio::test]
    async fn draft_product_can_be_activated_and_its_sku_updated() {
        let dao = MemoryCatalogDao::default();
        let product = dao
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Draft book".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Draft as i32,
                product_kind: pb::MallProductKind::Physical as i32,
                course_resource_id: String::new(),
                skus: vec![pb::CreateSkuRequest {
                    title: "Paper".to_string(),
                    price_cents: 100,
                    currency: "CNY".to_string(),
                    attributes: Default::default(),
                    saleable: true,
                }],
            })
            .await
            .expect("draft creation should return the management view");
        let sku_id = product.skus[0].id.clone();

        let updated = dao
            .update(pb::UpdateProductRequest {
                merchant_id: "merchant-a".to_string(),
                product_id: product.id.clone(),
                status: Some(pb::MallProductStatus::Active as i32),
                sku_updates: vec![pb::UpdateSkuRequest {
                    sku_id,
                    price_cents: Some(120),
                    saleable: Some(false),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .await
            .expect("product update should succeed");
        assert_eq!(updated.status, pb::MallProductStatus::Active as i32);
        assert_eq!(updated.skus[0].price_cents, 120);
        assert!(!updated.skus[0].saleable);
        assert!(matches!(
            dao.get(&product.id).await,
            Err(DaoError::NotFound(id)) if id == product.id
        ));
    }

    #[tokio::test]
    async fn merchant_catalog_is_isolated_and_management_lists_drafts() {
        let dao = MemoryCatalogDao::default();
        let product = dao
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Private draft".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Draft as i32,
                product_kind: pb::MallProductKind::Physical as i32,
                course_resource_id: String::new(),
                skus: vec![pb::CreateSkuRequest {
                    title: "Default".to_string(),
                    price_cents: 100,
                    currency: "CNY".to_string(),
                    attributes: Default::default(),
                    saleable: true,
                }],
            })
            .await
            .expect("product should be created");
        let denied = dao
            .update(pb::UpdateProductRequest {
                merchant_id: "merchant-b".to_string(),
                product_id: product.id.clone(),
                title: Some("Hijacked".to_string()),
                ..Default::default()
            })
            .await;
        assert!(matches!(denied, Err(DaoError::NotFound(id)) if id == product.id));
        let page = dao
            .list(pb::ProductQueryRequest {
                merchant_id: Some("merchant-a".to_string()),
                include_inactive: true,
                ..Default::default()
            })
            .await
            .expect("merchant should list own draft");
        assert_eq!(page.items, vec![product]);
    }

    #[tokio::test]
    async fn contextual_node_offer_is_idempotent_and_scoped_to_a_saleable_sku() {
        let dao = MemoryCatalogDao::default();
        let product = dao
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Trail kit".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Active as i32,
                product_kind: pb::MallProductKind::Physical as i32,
                course_resource_id: String::new(),
                skus: vec![
                    pb::CreateSkuRequest {
                        title: "Standard".to_string(),
                        price_cents: 1_000,
                        currency: "CNY".to_string(),
                        attributes: Default::default(),
                        saleable: true,
                    },
                    pb::CreateSkuRequest {
                        title: "Withdrawn".to_string(),
                        price_cents: 900,
                        currency: "CNY".to_string(),
                        attributes: Default::default(),
                        saleable: false,
                    },
                ],
            })
            .await
            .expect("product should be created");
        let request = pb::AttachNodeOfferRequest {
            merchant_id: "merchant-a".to_string(),
            product_id: product.id.clone(),
            sku_id: product.skus[0].id.clone(),
            route_id: "route-1".to_string(),
            action_node_id: "node-1".to_string(),
            creator_id: "creator-1".to_string(),
            commission_bps: 500,
            idempotency_key: "offer-key".to_string(),
            scene_equipment: "trail-running shoes".to_string(),
        };
        let first = dao
            .attach_node_offer(request.clone())
            .await
            .expect("offer should be attached");
        let retry = dao
            .attach_node_offer(request.clone())
            .await
            .expect("retry should return the same offer");
        assert_eq!(first.id, retry.id);
        assert_eq!(first.commission_bps, 500);
        assert_eq!(first.scene_equipment, "trail-running shoes");
        let mut conflicting = request.clone();
        conflicting.commission_bps = 700;
        assert!(matches!(
            dao.attach_node_offer(conflicting).await,
            Err(DaoError::Conflict(_))
        ));
        let other_context = dao
            .attach_node_offer(pb::AttachNodeOfferRequest {
                idempotency_key: "offer-key-other-context".to_string(),
                scene_equipment: "rain shell".to_string(),
                ..request.clone()
            })
            .await
            .expect("a distinct equipment context should have its own offer");
        let offers = dao
            .node_offers(pb::NodeOfferQueryRequest {
                route_id: "route-1".to_string(),
                action_node_id: "node-1".to_string(),
                limit: Some(10),
                scene_equipment: "trail-running shoes".to_string(),
            })
            .await
            .expect("node offers should be queryable");
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].id, first.id);
        assert_ne!(offers[0].id, other_context.id);
        assert_eq!(
            offers[0].product.as_ref().map(|product| &product.id),
            Some(&product.id)
        );
        assert!(
            offers[0]
                .product
                .as_ref()
                .is_some_and(|product| product.skus.iter().all(|sku| sku.saleable))
        );
        assert_eq!(
            offers[0].product.as_ref().map(|product| product.skus.len()),
            Some(1)
        );
    }

    #[tokio::test]
    async fn merchant_sku_ownership_only_matches_the_owning_merchant() {
        let dao = MemoryCatalogDao::default();
        let product = dao
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Owned SKU".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Active as i32,
                product_kind: pb::MallProductKind::Physical as i32,
                course_resource_id: String::new(),
                skus: vec![pb::CreateSkuRequest {
                    title: "Standard".to_string(),
                    price_cents: 1_000,
                    currency: "CNY".to_string(),
                    attributes: Default::default(),
                    saleable: true,
                }],
            })
            .await
            .expect("product should be created");
        let sku_id = product.skus[0].id.clone();

        let owned = dao
            .verify_merchant_sku(pb::MerchantSkuRequest {
                merchant_id: "merchant-a".to_string(),
                sku_id: sku_id.clone(),
            })
            .await
            .expect("ownership check should succeed");
        assert!(owned.owned);

        let foreign = dao
            .verify_merchant_sku(pb::MerchantSkuRequest {
                merchant_id: "merchant-b".to_string(),
                sku_id,
            })
            .await
            .expect("ownership check should succeed");
        assert!(!foreign.owned);

        let unknown = dao
            .verify_merchant_sku(pb::MerchantSkuRequest {
                merchant_id: "merchant-a".to_string(),
                sku_id: "sku-missing".to_string(),
            })
            .await
            .expect("ownership check should succeed");
        assert!(!unknown.owned);
    }
}

#[path = "memory_catalog_dao.rs"]
mod memory_catalog_dao;
pub(crate) use memory_catalog_dao::MemoryCatalogDao;
#[path = "postgres_catalog_dao.rs"]
mod postgres_catalog_dao;
pub(crate) use postgres_catalog_dao::PostgresCatalogDao;
