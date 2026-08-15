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
}

#[derive(Default)]
pub(crate) struct MemoryCatalogRepository {
    products: RwLock<HashMap<String, pb::MallProduct>>,
}
#[async_trait]
impl CatalogRepository for MemoryCatalogRepository {
    async fn create(
        &self,
        request: pb::CreateProductRequest,
    ) -> Result<pb::MallProduct, RepositoryError> {
        let product = new_product(request);
        self.products
            .write()
            .await
            .insert(product.id.clone(), product.clone());
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
        let mut values = self
            .products
            .read()
            .await
            .values()
            .filter(|product| product.status == pb::MallProductStatus::Active as i32)
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
        let product = new_product(request);
        let mut tx = self.pool.begin().await.map_err(database)?;
        sqlx::query("INSERT INTO mall_products (id,title,description,image_url,status) VALUES ($1,$2,$3,$4,$5)").bind(&product.id).bind(&product.title).bind(&product.description).bind(&product.image_url).bind(status_name(product.status)?).execute(&mut *tx).await.map_err(database)?;
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
            "UPDATE mall_products SET title=COALESCE($2,title),description=COALESCE($3,description),image_url=COALESCE($4,image_url),status=COALESCE($5,status),updated_at=now() WHERE id=$1",
        )
        .bind(id)
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
        let rows = sqlx::query_as::<_, ProductRow>("SELECT id,title,description,image_url,status,created_at,updated_at FROM mall_products WHERE status='active' AND id > $1 AND ($2='' OR title ILIKE '%' || $2 || '%') ORDER BY id LIMIT $3").bind(request.cursor.unwrap_or_default()).bind(request.query.unwrap_or_default()).bind(i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX)).fetch_all(&self.pool).await.map_err(database)?;
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
    use super::{CatalogRepository, MemoryCatalogRepository};
    use crate::api::pb;

    #[tokio::test]
    async fn draft_product_can_be_activated_and_its_sku_updated() {
        let repository = MemoryCatalogRepository::default();
        let product = repository
            .create(pb::CreateProductRequest {
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
}
