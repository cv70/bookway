use super::*;
use sqlx::{FromRow, PgPool, QueryBuilder};

#[derive(FromRow)]
struct ProductRow {
    id: String,
    title: String,
    description: String,
    image_url: String,
    status: String,
    product_kind: String,
    course_resource_id: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ProductRow {
    fn into_proto(self, skus: Vec<SkuRow>) -> Result<pb::MallProduct, DaoError> {
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
            product_kind: parse_kind(&self.product_kind)?,
            course_resource_id: self.course_resource_id,
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
    fn into_proto(self) -> Result<pb::MallSku, DaoError> {
        Ok(pb::MallSku {
            id: self.id,
            product_id: self.product_id,
            title: self.title,
            price_cents: self.price_cents,
            currency: self.currency,
            attributes: serde_json::from_value(self.attributes)
                .map_err(|error| DaoError::Failed(error.to_string()))?,
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

fn customer_product(mut product: pb::MallProduct) -> pb::MallProduct {
    product.skus.retain(|sku| sku.saleable);
    product
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

fn offer_row_matches_request(row: &NodeOfferRow, request: &pb::AttachNodeOfferRequest) -> bool {
    row.merchant_id == request.merchant_id
        && row.product_id == request.product_id
        && row.sku_id == request.sku_id
        && row.route_id == request.route_id
        && row.action_node_id == request.action_node_id
        && row.scene_equipment == request.scene_equipment
        && row.creator_id == request.creator_id
        && u32::try_from(row.commission_bps).unwrap_or_default() == request.commission_bps
}

#[derive(Clone)]
pub(crate) struct PostgresCatalogDao {
    pool: PgPool,
}

impl PostgresCatalogDao {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn load_product(&self, product: ProductRow) -> Result<pb::MallProduct, DaoError> {
        let skus = sqlx::query_as::<_, SkuRow>("SELECT id,product_id,title,price_cents,currency,attributes,saleable FROM mall_skus WHERE product_id=$1 ORDER BY id").bind(&product.id).fetch_all(&self.pool).await.map_err(database)?;
        product.into_proto(skus)
    }
    async fn load_internal(&self, id: &str) -> Result<pb::MallProduct, DaoError> {
        let product = sqlx::query_as::<_, ProductRow>(
            "SELECT id,title,description,image_url,status,product_kind,course_resource_id,created_at,updated_at FROM mall_products WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        self.load_product(product).await
    }

    async fn load_customer(&self, id: &str) -> Result<pb::MallProduct, DaoError> {
        let product = sqlx::query_as::<_, ProductRow>(
            "SELECT id,title,description,image_url,status,product_kind,course_resource_id,created_at,updated_at FROM mall_products WHERE id=$1 AND status='active' AND EXISTS (SELECT 1 FROM mall_skus WHERE product_id=mall_products.id AND saleable=true)",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        let skus = sqlx::query_as::<_, SkuRow>(
            "SELECT id,product_id,title,price_cents,currency,attributes,saleable FROM mall_skus WHERE product_id=$1 AND saleable=true ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        product.into_proto(skus)
    }
}

#[async_trait]
impl CatalogDao for PostgresCatalogDao {
    async fn create(&self, request: pb::CreateProductRequest) -> Result<pb::MallProduct, DaoError> {
        let merchant_id = request.merchant_id.clone();
        let product = new_product(request);
        let mut tx = self.pool.begin().await.map_err(database)?;
        sqlx::query("INSERT INTO mall_products (id,merchant_id,title,description,image_url,status,product_kind,course_resource_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(&product.id).bind(&merchant_id).bind(&product.title).bind(&product.description).bind(&product.image_url).bind(status_name(product.status)?).bind(kind_name(product.product_kind)?).bind(&product.course_resource_id).execute(&mut *tx).await.map_err(database)?;
        for sku in &product.skus {
            sqlx::query("INSERT INTO mall_skus (id,product_id,title,price_cents,currency,attributes,saleable) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(&sku.id).bind(&sku.product_id).bind(&sku.title).bind(sku.price_cents).bind(&sku.currency).bind(serde_json::to_value(&sku.attributes).map_err(|error| DaoError::Failed(error.to_string()))?).bind(sku.saleable).execute(&mut *tx).await.map_err(database)?;
        }
        tx.commit().await.map_err(database)?;
        // Management creates draft products before they are customer-visible.
        self.load_internal(&product.id).await
    }
    async fn update(&self, request: pb::UpdateProductRequest) -> Result<pb::MallProduct, DaoError> {
        let id = &request.product_id;
        let mut transaction = self.pool.begin().await.map_err(database)?;
        let changed = sqlx::query(
            "UPDATE mall_products SET title=COALESCE($3,title),description=COALESCE($4,description),image_url=COALESCE($5,image_url),status=COALESCE($6,status),product_kind=COALESCE($7,product_kind),course_resource_id=COALESCE($8,course_resource_id),updated_at=now() WHERE id=$1 AND merchant_id=$2",
        )
        .bind(id)
        .bind(&request.merchant_id)
        .bind(request.title)
        .bind(request.description)
        .bind(request.image_url)
        .bind(request.status.map(status_name).transpose()?)
        .bind(request
            .product_kind
            .map(kind_name)
            .transpose()?
            .map(str::to_string))
        .bind(request.course_resource_id)
        .execute(&mut *transaction)
        .await
        .map_err(database)?
        .rows_affected();
        if changed == 0 {
            return Err(DaoError::NotFound(id.to_string()));
        }
        for sku in request.sku_updates {
            let attributes = sku
                .attributes
                .map(|value| {
                    serde_json::to_value(value.values)
                        .map_err(|error| DaoError::Failed(error.to_string()))
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
                return Err(DaoError::NotFound(sku.sku_id));
            }
        }
        transaction.commit().await.map_err(database)?;
        self.load_internal(id).await
    }
    async fn list(&self, request: pb::ProductQueryRequest) -> Result<pb::ProductPage, DaoError> {
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 100)).unwrap_or(100);
        let rows = sqlx::query_as::<_, ProductRow>("SELECT id,title,description,image_url,status,product_kind,course_resource_id,created_at,updated_at FROM mall_products WHERE id > $1 AND ($2='' OR title ILIKE '%' || $2 || '%') AND ($3='' OR merchant_id=$3) AND ($4 OR (status='active' AND EXISTS (SELECT 1 FROM mall_skus WHERE product_id=mall_products.id AND saleable=true))) ORDER BY id LIMIT $5").bind(request.cursor.unwrap_or_default()).bind(request.query.unwrap_or_default()).bind(request.merchant_id.unwrap_or_default()).bind(request.include_inactive).bind(i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX)).fetch_all(&self.pool).await.map_err(database)?;
        let more = rows.len() > limit;
        let mut values = Vec::with_capacity(rows.len().min(limit));
        for row in rows.into_iter().take(limit) {
            let product = self.load_product(row).await?;
            values.push(if request.include_inactive {
                product
            } else {
                customer_product(product)
            });
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
    async fn get(&self, id: &str) -> Result<pb::MallProduct, DaoError> {
        self.load_customer(id).await
    }
    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, DaoError> {
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
    ) -> Result<pb::NodeOffer, DaoError> {
        let owns_product = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM mall_products WHERE id=$1 AND merchant_id=$2)",
        )
        .bind(&request.product_id)
        .bind(&request.merchant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(database)?;
        if !owns_product {
            return Err(DaoError::NotFound(request.product_id));
        }
        let id = Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, NodeOfferRow>(
            "INSERT INTO mall_node_offers (id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (idempotency_key) DO NOTHING RETURNING id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,created_at",
        )
        .bind(id)
        .bind(&request.merchant_id)
        .bind(&request.product_id)
        .bind(&request.sku_id)
        .bind(&request.route_id)
        .bind(&request.action_node_id)
        .bind(&request.scene_equipment)
        .bind(&request.creator_id)
        .bind(i32::try_from(request.commission_bps).map_err(|_| DaoError::Failed("commission exceeds supported range".to_string()))?)
        .bind(&request.idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        if let Some(row) = row {
            return Ok(row.into_proto());
        }
        let existing = sqlx::query_as::<_, NodeOfferRow>(
            "SELECT id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,created_at FROM mall_node_offers WHERE idempotency_key=$1",
        )
        .bind(&request.idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::Failed("node offer idempotency row disappeared".to_string()))?;
        if !offer_row_matches_request(&existing, &request) {
            return Err(DaoError::Conflict(
                "idempotency key is already bound to a different node offer".to_string(),
            ));
        }
        Ok(existing.into_proto())
    }
    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, DaoError> {
        let limit = i64::from(request.limit.unwrap_or(20).clamp(1, 50));
        let rows = sqlx::query_as::<_, NodeOfferRow>(
            "SELECT o.id,o.merchant_id,o.product_id,o.sku_id,o.route_id,o.action_node_id,o.scene_equipment,o.creator_id,o.commission_bps,o.created_at FROM mall_node_offers o INNER JOIN mall_products p ON p.id=o.product_id INNER JOIN mall_skus s ON s.id=o.sku_id AND s.product_id=o.product_id WHERE o.route_id=$1 AND o.action_node_id=$2 AND o.scene_equipment=$3 AND p.status='active' AND s.saleable=true ORDER BY o.id LIMIT $4",
        )
        .bind(request.route_id)
        .bind(request.action_node_id)
        .bind(request.scene_equipment)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        let mut offers = Vec::with_capacity(rows.len());
        for row in rows {
            let product = self.load_customer(&row.product_id).await?;
            let mut offer = row.into_proto();
            offer.product = Some(product);
            offers.push(offer);
        }
        Ok(offers)
    }
    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, DaoError> {
        // Internal order settlement needs the original route association even
        // after a SKU is withdrawn; customer-facing NodeOffers remains active-only.
        let row = sqlx::query_as::<_, NodeOfferRow>(
            "SELECT id,merchant_id,product_id,sku_id,route_id,action_node_id,scene_equipment,creator_id,commission_bps,created_at FROM mall_node_offers WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        Ok(row.into_proto())
    }
    async fn verify_merchant_sku(
        &self,
        request: pb::MerchantSkuRequest,
    ) -> Result<pb::MerchantSkuDecision, DaoError> {
        let owned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM mall_skus s INNER JOIN mall_products p ON p.id=s.product_id WHERE s.id=$1 AND p.merchant_id=$2)",
        )
        .bind(&request.sku_id)
        .bind(&request.merchant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(database)?;
        Ok(pb::MerchantSkuDecision { owned })
    }
}
