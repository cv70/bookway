use crate::api::pb;
use async_trait::async_trait;
use sqlx::{FromRow, PgPool, QueryBuilder};
use std::collections::HashMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) enum RepositoryError {
    NotFound(String),
    Failed(String),
}
#[async_trait]
pub(crate) trait CatalogRepository: Send + Sync {
    async fn create(
        &self,
        request: pb::CreateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError>;
    async fn update(
        &self,
        request: pb::UpdateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError>;
    async fn list(
        &self,
        request: pb::ProductQueryRequest,
    ) -> Result<pb::ProductPage, RepositoryError>;
    async fn get(&self, id: &str) -> Result<pb::MallProduct, RepositoryError>;
    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, RepositoryError>;
    async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, RepositoryError>;
    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, RepositoryError>;
    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, RepositoryError>;
}

#[derive(Default)]
pub(crate) struct MemoryCatalogRepository {
    products: RwLock<HashMap<String, pb::MallProduct>>,
    product_merchants: RwLock<HashMap<String, String>>,
    node_offers: RwLock<HashMap<String, pb::NodeOffer>>,
    node_offer_idempotency: RwLock<HashMap<String, String>>,
}
#[async_trait]
impl CatalogRepository for MemoryCatalogRepository {
    async fn create(
        &self,
        request: pb::CreateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError> {
        let merchant_id = request.merchant_id.clone();
        let product = new_product(request);
        self.products
            .write()
            .await
            .insert(product.id.clone(), product.clone());
        self.product_merchants
            .write()
            .await
            .insert(product.id.clone(), merchant_id);
        Ok(product)
    }
    async fn update(
        &self,
        request: pb::UpdateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError> {
        let mut products = self.products.write().await;
        let product = products
            .get_mut(&request.product_id)
            .ok_or_else(|| RepositoryError::NotFound(request.product_id.clone()))?;
        let owner = self
            .product_merchants
            .read()
            .await
            .get(&request.product_id)
            .cloned();
        if owner.as_deref() != Some(request.merchant_id.as_str()) {
            return Err(RepositoryError::NotFound(request.product_id));
        }
        apply_update(product, request)?;
        Ok(product.clone())
    }
    async fn list(
        &self,
        request: pb::ProductQueryRequest,
    ) -> Result<pb::ProductPage, RepositoryError> {
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 100)).unwrap_or(100);
        let cursor = request.cursor.unwrap_or_default();
        let query = request.query.unwrap_or_default().to_lowercase();
        let merchant_id = request.merchant_id.unwrap_or_default();
        let product_merchants = self.product_merchants.read().await.clone();
        let mut values = self
            .products
            .read()
            .await
            .values()
            .filter(|product| {
                request.include_inactive || product.status == pb::MallProductStatus::Active as i32
            })
            .filter(|product| {
                merchant_id.is_empty()
                    || product_merchants
                        .get(&product.id)
                        .is_some_and(|owner| owner == &merchant_id)
            })
            .filter(|product| product.id > cursor)
            .filter(|product| query.is_empty() || product.title.to_lowercase().contains(&query))
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        let next_cursor = if values.len() > limit {
            Some(values[limit - 1].id.clone())
        } else {
            None
        };
        values.truncate(limit);
        Ok(pb::ProductPage {
            items: values,
            next_cursor,
        })
    }
    async fn get(&self, id: &str) -> Result<pb::MallProduct, RepositoryError> {
        self.products
            .read()
            .await
            .get(id)
            .cloned()
            .filter(|product| product.status == pb::MallProductStatus::Active as i32)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }
    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, RepositoryError> {
        let ids = ids.into_iter().collect::<std::collections::HashSet<_>>();
        let mut values = self
            .products
            .read()
            .await
            .values()
            .filter(|product| product.status == pb::MallProductStatus::Active as i32)
            .flat_map(|product| product.skus.iter())
            .filter(|sku| sku.saleable && ids.contains(&sku.id))
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(values)
    }
    async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, RepositoryError> {
        let owner = self
            .product_merchants
            .read()
            .await
            .get(&request.product_id)
            .cloned();
        if owner.as_deref() != Some(request.merchant_id.as_str()) {
            return Err(RepositoryError::NotFound(request.product_id));
        }
        let mut idempotency = self.node_offer_idempotency.write().await;
        if let Some(id) = idempotency.get(&request.idempotency_key) {
            return self
                .node_offers
                .read()
                .await
                .get(id)
                .cloned()
                .ok_or_else(|| {
                    RepositoryError::Failed("missing node offer idempotency target".to_string())
                });
        }
        let offer = pb::NodeOffer {
            id: Uuid::now_v7().to_string(),
            product_id: request.product_id,
            sku_id: request.sku_id,
            route_id: request.route_id,
            action_node_id: request.action_node_id,
            creator_id: request.creator_id,
            commission_bps: request.commission_bps,
            created_at: timestamp(OffsetDateTime::now_utc()),
            scene_equipment: request.scene_equipment,
            product: None,
            merchant_id: request.merchant_id,
        };
        idempotency.insert(request.idempotency_key, offer.id.clone());
        self.node_offers
            .write()
            .await
            .insert(offer.id.clone(), offer.clone());
        Ok(offer)
    }
    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, RepositoryError> {
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 50)).unwrap_or(50);
        let products = self.products.read().await;
        let mut offers = self
            .node_offers
            .read()
            .await
            .values()
            .filter(|offer| offer.route_id == request.route_id)
            .filter(|offer| offer.action_node_id == request.action_node_id)
            .filter_map(|offer| {
                let product = products.get(&offer.product_id)?;
                let saleable = product.status == pb::MallProductStatus::Active as i32
                    && product
                        .skus
                        .iter()
                        .any(|sku| sku.id == offer.sku_id && sku.saleable);
                saleable.then(|| {
                    let mut offer = offer.clone();
                    offer.product = Some(product.clone());
                    offer
                })
            })
            .collect::<Vec<_>>();
        offers.sort_by(|left, right| left.id.cmp(&right.id));
        offers.truncate(limit);
        Ok(offers)
    }
    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, RepositoryError> {
        self.node_offers
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }
}

#[derive(Clone)]
pub(crate) struct PostgresCatalogRepository {
    pool: PgPool,
}
impl PostgresCatalogRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn load_product(&self, product: ProductRow) -> Result<pb::MallProduct, RepositoryError> {
        let skus = sqlx::query_as::<_, SkuRow>("SELECT id,product_id,title,price_cents,currency,attributes,saleable FROM mall_skus WHERE product_id=$1 ORDER BY id").bind(&product.id).fetch_all(&self.pool).await.map_err(database)?;
        product.into_proto(skus)
    }
    async fn load_internal(&self, id: &str) -> Result<pb::MallProduct, RepositoryError> {
        let product = sqlx::query_as::<_, ProductRow>(
            "SELECT id,title,description,image_url,status,created_at,updated_at FROM mall_products WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        self.load_product(product).await
    }
}
#[async_trait]
impl CatalogRepository for PostgresCatalogRepository {
    async fn create(
        &self,
        request: pb::CreateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError> {
        let merchant_id = request.merchant_id.clone();
        let product = new_product(request);
        let mut tx = self.pool.begin().await.map_err(database)?;
        sqlx::query("INSERT INTO mall_products (id,merchant_id,title,description,image_url,status) VALUES ($1,$2,$3,$4,$5,$6)").bind(&product.id).bind(&merchant_id).bind(&product.title).bind(&product.description).bind(&product.image_url).bind(status_name(product.status)?).execute(&mut *tx).await.map_err(database)?;
        for sku in &product.skus {
            sqlx::query("INSERT INTO mall_skus (id,product_id,title,price_cents,currency,attributes,saleable) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(&sku.id).bind(&sku.product_id).bind(&sku.title).bind(sku.price_cents).bind(&sku.currency).bind(serde_json::to_value(&sku.attributes).map_err(|error| RepositoryError::Failed(error.to_string()))?).bind(sku.saleable).execute(&mut *tx).await.map_err(database)?;
        }
        tx.commit().await.map_err(database)?;
        // Management creates draft products before they are customer-visible.
        self.load_internal(&product.id).await
    }
    async fn update(
        &self,
        request: pb::UpdateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError> {
        let id = &request.product_id;
        let mut transaction = self.pool.begin().await.map_err(database)?;
        let changed = sqlx::query(
            "UPDATE mall_products SET title=COALESCE($3,title),description=COALESCE($4,description),image_url=COALESCE($5,image_url),status=COALESCE($6,status),updated_at=now() WHERE id=$1 AND merchant_id=$2",
        )
        .bind(id)
        .bind(&request.merchant_id)
        .bind(request.title)
        .bind(request.description)
        .bind(request.image_url)
        .bind(request.status.map(status_name).transpose()?)
        .execute(&mut *transaction)
        .await
        .map_err(database)?
        .rows_affected();
        if changed == 0 {
            return Err(RepositoryError::NotFound(id.to_string()));
        }
        for sku in request.sku_updates {
            let attributes = sku
                .attributes
                .map(|value| {
                    serde_json::to_value(value.values)
                        .map_err(|error| RepositoryError::Failed(error.to_string()))
                })
                .transpose()?;
            let changed = sqlx::query(
                "UPDATE mall_skus SET title=COALESCE($3,title),price_cents=COALESCE($4,price_cents),currency=COALESCE($5,currency),attributes=COALESCE($6,attributes),saleable=COALESCE($7,saleable) WHERE id=$1 AND product_id=$2",
            )
            .bind(&sku.sku_id)
            .bind(id)
            .bind(sku.title)
            .bind(sku.price_cents)
            .bind(sku.currency)
            .bind(attributes)
            .bind(sku.saleable)
            .execute(&mut *transaction)
            .await
            .map_err(database)?
            .rows_affected();
            if changed == 0 {
                return Err(RepositoryError::NotFound(sku.sku_id));
            }
        }
        transaction.commit().await.map_err(database)?;
        self.load_internal(id).await
    }
    async fn list(
        &self,
        request: pb::ProductQueryRequest,
    ) -> Result<pb::ProductPage, RepositoryError> {
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 100)).unwrap_or(100);
        let rows = sqlx::query_as::<_, ProductRow>("SELECT id,title,description,image_url,status,created_at,updated_at FROM mall_products WHERE id > $1 AND ($2='' OR title ILIKE '%' || $2 || '%') AND ($3='' OR merchant_id=$3) AND ($4 OR status='active') ORDER BY id LIMIT $5").bind(request.cursor.unwrap_or_default()).bind(request.query.unwrap_or_default()).bind(request.merchant_id.unwrap_or_default()).bind(request.include_inactive).bind(i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX)).fetch_all(&self.pool).await.map_err(database)?;
        let more = rows.len() > limit;
        let mut values = Vec::with_capacity(rows.len().min(limit));
        for row in rows.into_iter().take(limit) {
            values.push(self.load_product(row).await?);
        }
        let next_cursor = if more {
            values.last().map(|value| value.id.clone())
        } else {
            None
        };
        Ok(pb::ProductPage {
            items: values,
            next_cursor,
        })
    }
    async fn get(&self, id: &str) -> Result<pb::MallProduct, RepositoryError> {
        let row = sqlx::query_as::<_, ProductRow>("SELECT id,title,description,image_url,status,created_at,updated_at FROM mall_products WHERE id=$1 AND status='active'").bind(id).fetch_optional(&self.pool).await.map_err(database)?.ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        self.load_product(row).await
    }
    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, RepositoryError> {
        let mut builder = QueryBuilder::<sqlx::Postgres>::new(
            "SELECT s.id,s.product_id,s.title,s.price_cents,s.currency,s.attributes,s.saleable FROM mall_skus s INNER JOIN mall_products p ON p.id=s.product_id WHERE p.status='active' AND s.saleable=true AND s.id IN (",
        );
        let mut values = builder.separated(",");
        for id in ids {
            values.push_bind(id);
        }
        values.push_unseparated(") ORDER BY s.id");
        let rows = builder
            .build_query_as::<SkuRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(database)?;
        rows.into_iter().map(SkuRow::into_proto).collect()
    }
    async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, RepositoryError> {
        let owns_product = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM mall_products WHERE id=$1 AND merchant_id=$2)",
        )
        .bind(&request.product_id)
        .bind(&request.merchant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(database)?;
        if !owns_product {
            return Err(RepositoryError::NotFound(request.product_id));
        }
        let id = Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, NodeOfferRow>(
            "INSERT INTO mall_node_offers (id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (idempotency_key) DO UPDATE SET idempotency_key=EXCLUDED.idempotency_key RETURNING id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,created_at",
        )
        .bind(id)
        .bind(request.merchant_id)
        .bind(request.product_id)
        .bind(request.sku_id)
        .bind(request.route_id)
        .bind(request.action_node_id)
        .bind(request.scene_equipment)
        .bind(request.creator_id)
        .bind(i32::try_from(request.commission_bps).map_err(|_| RepositoryError::Failed("commission exceeds supported range".to_string()))?)
        .bind(request.idempotency_key)
        .fetch_one(&self.pool)
        .await
        .map_err(database)?;
        Ok(row.into_proto())
    }
    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, RepositoryError> {
        let limit = i64::from(request.limit.unwrap_or(20).clamp(1, 50));
        let rows = sqlx::query_as::<_, NodeOfferRow>(
            "SELECT o.id,o.merchant_id,o.product_id,o.sku_id,o.route_id,o.action_node_id,o.scene_equipment,o.creator_id,o.commission_bps,o.created_at FROM mall_node_offers o INNER JOIN mall_products p ON p.id=o.product_id INNER JOIN mall_skus s ON s.id=o.sku_id AND s.product_id=o.product_id WHERE o.route_id=$1 AND o.action_node_id=$2 AND p.status='active' AND s.saleable=true ORDER BY o.id LIMIT $3",
        )
        .bind(request.route_id)
        .bind(request.action_node_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        let mut offers = Vec::with_capacity(rows.len());
        for row in rows {
            let product = self.load_internal(&row.product_id).await?;
            let mut offer = row.into_proto();
            offer.product = Some(product);
            offers.push(offer);
        }
        Ok(offers)
    }
    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, RepositoryError> {
        // Internal order settlement needs the original route association even
        // after a SKU is withdrawn; customer-facing NodeOffers remains active-only.
        let row = sqlx::query_as::<_, NodeOfferRow>(
            "SELECT id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,created_at FROM mall_node_offers WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        Ok(row.into_proto())
    }
}
#[derive(FromRow)]
struct ProductRow {
    id: String,
    title: String,
    description: String,
    image_url: String,
    status: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
impl ProductRow {
    fn into_proto(self, skus: Vec<SkuRow>) -> Result<pb::MallProduct, RepositoryError> {
        Ok(pb::MallProduct {
            id: self.id,
            title: self.title,
            description: self.description,
            image_url: self.image_url,
            status: parse_status(&self.status)?,
            skus: skus
                .into_iter()
                .map(SkuRow::into_proto)
                .collect::<Result<_, _>>()?,
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        })
    }
}
#[derive(FromRow)]
struct SkuRow {
    id: String,
    product_id: String,
    title: String,
    price_cents: i64,
    currency: String,
    attributes: serde_json::Value,
    saleable: bool,
}
impl SkuRow {
    fn into_proto(self) -> Result<pb::MallSku, RepositoryError> {
        Ok(pb::MallSku {
            id: self.id,
            product_id: self.product_id,
            title: self.title,
            price_cents: self.price_cents,
            currency: self.currency,
            attributes: serde_json::from_value(self.attributes)
                .map_err(|error| RepositoryError::Failed(error.to_string()))?,
            saleable: self.saleable,
        })
    }
}
#[derive(FromRow)]
struct NodeOfferRow {
    id: String,
    merchant_id: String,
    product_id: String,
    sku_id: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
    creator_id: String,
    commission_bps: i32,
    created_at: OffsetDateTime,
}
impl NodeOfferRow {
    fn into_proto(self) -> pb::NodeOffer {
        pb::NodeOffer {
            id: self.id,
            merchant_id: self.merchant_id,
            product_id: self.product_id,
            sku_id: self.sku_id,
            route_id: self.route_id,
            action_node_id: self.action_node_id,
            creator_id: self.creator_id,
            commission_bps: self.commission_bps.max(0) as u32,
            created_at: timestamp(self.created_at),
            scene_equipment: self.scene_equipment,
            product: None,
        }
    }
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
        created_at: now.clone(),
        updated_at: now,
    }
}
fn apply_update(
    product: &mut pb::MallProduct,
    request: pb::UpdateProductRequest,
) -> Result<(), RepositoryError> {
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
    for update in request.sku_updates {
        let sku = product
            .skus
            .iter_mut()
            .find(|sku| sku.id == update.sku_id)
            .ok_or_else(|| RepositoryError::NotFound(update.sku_id.clone()))?;
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
fn status_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::MallProductStatus::try_from(value).ok() {
        Some(pb::MallProductStatus::Draft) => Ok("draft"),
        Some(pb::MallProductStatus::Active) => Ok("active"),
        Some(pb::MallProductStatus::Archived) => Ok("archived"),
        None => Err(RepositoryError::Failed(
            "invalid product status".to_string(),
        )),
    }
}
fn parse_status(value: &str) -> Result<i32, RepositoryError> {
    match value {
        "draft" => Ok(pb::MallProductStatus::Draft as i32),
        "active" => Ok(pb::MallProductStatus::Active as i32),
        "archived" => Ok(pb::MallProductStatus::Archived as i32),
        _ => Err(RepositoryError::Failed(format!(
            "unknown product status {value}"
        ))),
    }
}
fn database(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Failed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{CatalogRepository, MemoryCatalogRepository, RepositoryError};
    use crate::api::pb;

    #[tokio::test]
    async fn draft_product_can_be_activated_and_its_sku_updated() {
        let repository = MemoryCatalogRepository::default();
        let product = repository
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Draft book".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Draft as i32,
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

        let updated = repository
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
        assert_eq!(
            repository
                .get(&product.id)
                .await
                .expect("active product should be public")
                .id,
            product.id
        );
    }

    #[tokio::test]
    async fn merchant_catalog_is_isolated_and_management_lists_drafts() {
        let repository = MemoryCatalogRepository::default();
        let product = repository
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Private draft".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Draft as i32,
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
        let denied = repository
            .update(pb::UpdateProductRequest {
                merchant_id: "merchant-b".to_string(),
                product_id: product.id.clone(),
                title: Some("Hijacked".to_string()),
                ..Default::default()
            })
            .await;
        assert!(matches!(denied, Err(RepositoryError::NotFound(id)) if id == product.id));
        let page = repository
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
        let repository = MemoryCatalogRepository::default();
        let product = repository
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Trail kit".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Active as i32,
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
        let first = repository
            .attach_node_offer(request.clone())
            .await
            .expect("offer should be attached");
        let retry = repository
            .attach_node_offer(request)
            .await
            .expect("retry should return the same offer");
        assert_eq!(first.id, retry.id);
        assert_eq!(first.commission_bps, 500);
        assert_eq!(first.scene_equipment, "trail-running shoes");
        let offers = repository
            .node_offers(pb::NodeOfferQueryRequest {
                route_id: "route-1".to_string(),
                action_node_id: "node-1".to_string(),
                limit: Some(10),
            })
            .await
            .expect("node offers should be queryable");
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].id, first.id);
        assert_eq!(
            offers[0].product.as_ref().map(|product| &product.id),
            Some(&product.id)
        );
    }
}
