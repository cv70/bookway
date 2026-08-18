use crate::api::pb;
use async_trait::async_trait;
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

#[derive(Debug)]
pub(crate) enum RepositoryError {
    NotFound(String),
    Conflict(String),
    State(String),
    Failed(String),
}
#[derive(Clone)]
pub(crate) struct NewOrder {
    pub(crate) order: pb::Order,
    pub(crate) idempotency_key: String,
    pub(crate) request_fingerprint: String,
}
pub(crate) struct CreateResult {
    pub(crate) order: pb::Order,
}
#[async_trait]
pub(crate) trait OrderRepository: Send + Sync {
    async fn find_idempotent(
        &self,
        user_id: &str,
        key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Order>, RepositoryError>;
    async fn create(&self, draft: NewOrder) -> Result<CreateResult, RepositoryError>;
    async fn get(&self, user_id: &str, id: &str) -> Result<pb::Order, RepositoryError>;
    async fn list(&self, user_id: &str) -> Result<Vec<pb::Order>, RepositoryError>;
    async fn expired_pending(&self, limit: usize) -> Result<Vec<pb::Order>, RepositoryError>;
    async fn claim_payment_reference(
        &self,
        id: &str,
        payment_reference: &str,
    ) -> Result<pb::Order, RepositoryError>;
    async fn transition(
        &self,
        id: &str,
        status: i32,
        payment_reference: Option<String>,
    ) -> Result<pb::Order, RepositoryError>;
}
#[derive(Default)]
pub(crate) struct MemoryOrderRepository {
    orders: RwLock<HashMap<String, MemoryOrder>>,
    idempotency: RwLock<HashMap<(String, String), String>>,
    payment_references: RwLock<HashMap<String, String>>,
}
#[derive(Clone)]
struct MemoryOrder {
    order: pb::Order,
    request_fingerprint: String,
}
#[async_trait]
impl OrderRepository for MemoryOrderRepository {
    async fn find_idempotent(
        &self,
        user_id: &str,
        key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Order>, RepositoryError> {
        let index = self.idempotency.read().await;
        let Some(id) = index.get(&(user_id.to_string(), key.to_string())) else {
            return Ok(None);
        };
        let order = self.orders.read().await.get(id).cloned();
        match order {
            Some(order) if order.request_fingerprint == request_fingerprint => {
                Ok(Some(order.order))
            }
            Some(_) => Err(RepositoryError::Conflict(
                "Idempotency-Key was already used with a different order".to_string(),
            )),
            None => Err(RepositoryError::Failed(
                "missing idempotency target".to_string(),
            )),
        }
    }
    async fn create(&self, draft: NewOrder) -> Result<CreateResult, RepositoryError> {
        let mut index = self.idempotency.write().await;
        if let Some(id) = index.get(&(draft.order.user_id.clone(), draft.idempotency_key.clone())) {
            let stored = self
                .orders
                .read()
                .await
                .get(id)
                .ok_or_else(|| RepositoryError::Failed("missing idempotency target".to_string()))?
                .clone();
            if stored.request_fingerprint != draft.request_fingerprint {
                return Err(RepositoryError::Conflict(
                    "Idempotency-Key was already used with a different order".to_string(),
                ));
            }
            return Ok(CreateResult {
                order: stored.order,
            });
        }
        let order = draft.order.clone();
        index.insert(
            (order.user_id.clone(), draft.idempotency_key.clone()),
            order.id.clone(),
        );
        self.orders.write().await.insert(
            order.id.clone(),
            MemoryOrder {
                order: order.clone(),
                request_fingerprint: draft.request_fingerprint,
            },
        );
        Ok(CreateResult { order })
    }
    async fn get(&self, user_id: &str, id: &str) -> Result<pb::Order, RepositoryError> {
        self.orders
            .read()
            .await
            .get(id)
            .filter(|order| order.order.user_id == user_id)
            .map(|order| order.order.clone())
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }
    async fn list(&self, user_id: &str) -> Result<Vec<pb::Order>, RepositoryError> {
        let mut values = self
            .orders
            .read()
            .await
            .values()
            .filter(|order| order.order.user_id == user_id)
            .map(|order| order.order.clone())
            .collect::<Vec<_>>();
        values.sort_by(|left, right| right.id.cmp(&left.id));
        Ok(values)
    }
    async fn expired_pending(&self, limit: usize) -> Result<Vec<pb::Order>, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        let mut values = self
            .orders
            .read()
            .await
            .values()
            .filter(|order| {
                order.order.status == pb::MallOrderStatus::PendingPayment as i32
                    && OffsetDateTime::parse(&order.order.expires_at, &Rfc3339)
                        .is_ok_and(|value| value <= now)
            })
            .map(|order| order.order.clone())
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.expires_at.cmp(&right.expires_at));
        values.truncate(limit);
        Ok(values)
    }
    async fn claim_payment_reference(
        &self,
        id: &str,
        payment_reference: &str,
    ) -> Result<pb::Order, RepositoryError> {
        let mut orders = self.orders.write().await;
        let order = orders
            .get_mut(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        if order.order.status == pb::MallOrderStatus::Paid as i32 {
            if order.order.payment_reference.as_deref() == Some(payment_reference) {
                return Ok(order.order.clone());
            }
            return Err(RepositoryError::Conflict(
                "payment reference belongs to a different order".to_string(),
            ));
        }
        if order.order.status != pb::MallOrderStatus::PendingPayment as i32 {
            return Err(RepositoryError::Failed(format!(
                "order {id} is not pending payment"
            )));
        }
        if OffsetDateTime::parse(&order.order.expires_at, &Rfc3339)
            .is_ok_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
        {
            return Err(RepositoryError::State(
                "order payment window has expired".to_string(),
            ));
        }
        if let Some(existing) = &order.order.payment_reference {
            if existing != payment_reference {
                return Err(RepositoryError::Conflict(
                    "order already has a different payment reference".to_string(),
                ));
            }
            return Ok(order.order.clone());
        }
        let mut references = self.payment_references.write().await;
        if let Some(owner) = references.get(payment_reference)
            && owner != id
        {
            return Err(RepositoryError::Conflict(
                "payment reference belongs to a different order".to_string(),
            ));
        }
        references.insert(payment_reference.to_string(), id.to_string());
        order.order.payment_reference = Some(payment_reference.to_string());
        order.order.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(order.order.clone())
    }
    async fn transition(
        &self,
        id: &str,
        status: i32,
        payment_reference: Option<String>,
    ) -> Result<pb::Order, RepositoryError> {
        let mut orders = self.orders.write().await;
        let order = orders
            .get_mut(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        if order.order.status == status {
            if status == pb::MallOrderStatus::Paid as i32
                && payment_reference.is_some()
                && order.order.payment_reference != payment_reference
            {
                return Err(RepositoryError::Conflict(
                    "payment reference belongs to a different payment".to_string(),
                ));
            }
            return Ok(order.order.clone());
        }
        if order.order.status != pb::MallOrderStatus::PendingPayment as i32 {
            return Err(RepositoryError::Failed(format!(
                "order {id} is not pending payment"
            )));
        }
        if status == pb::MallOrderStatus::Paid as i32 {
            let reference = payment_reference
                .or_else(|| order.order.payment_reference.clone())
                .ok_or_else(|| {
                    RepositoryError::Failed("paid orders require a payment reference".to_string())
                })?;
            let mut references = self.payment_references.write().await;
            if let Some(owner) = references.get(&reference)
                && owner != id
            {
                return Err(RepositoryError::Conflict(
                    "payment reference belongs to a different order".to_string(),
                ));
            }
            references.insert(reference.clone(), id.to_string());
            order.order.payment_reference = Some(reference);
        } else if payment_reference.is_some() {
            return Err(RepositoryError::Failed(
                "only paid orders can carry a payment reference".to_string(),
            ));
        }
        order.order.status = status;
        order.order.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(order.order.clone())
    }
}
#[derive(Clone)]
pub(crate) struct PostgresOrderRepository {
    pool: PgPool,
}
impl PostgresOrderRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn load(&self, user_id: Option<&str>, id: &str) -> Result<pb::Order, RepositoryError> {
        let row = if let Some(user_id) = user_id {
            sqlx::query_as::<_, OrderRow>("SELECT id,user_id,status,currency,total_cents,payment_reference,expires_at,created_at,updated_at,node_offer_id,affiliate_creator_id,commission_cents FROM mall_orders WHERE id=$1 AND user_id=$2").bind(id).bind(user_id).fetch_optional(&self.pool).await.map_err(database)?
        } else {
            sqlx::query_as::<_, OrderRow>("SELECT id,user_id,status,currency,total_cents,payment_reference,expires_at,created_at,updated_at,node_offer_id,affiliate_creator_id,commission_cents FROM mall_orders WHERE id=$1").bind(id).fetch_optional(&self.pool).await.map_err(database)?
        };
        let row = row.ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        let items = sqlx::query_as::<_, OrderLineRow>("SELECT sku_id,product_id,title,quantity,unit_price_cents,currency,line_total_cents FROM mall_order_items WHERE order_id=$1 ORDER BY sku_id").bind(id).fetch_all(&self.pool).await.map_err(database)?;
        order_from_row(row, items)
    }
}
#[async_trait]
impl OrderRepository for PostgresOrderRepository {
    async fn find_idempotent(
        &self,
        user_id: &str,
        key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Order>, RepositoryError> {
        let row = sqlx::query_as::<_, IdempotencyRow>(
            "SELECT id,request_fingerprint FROM mall_orders WHERE user_id=$1 AND idempotency_key=$2",
        )
        .bind(user_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        match row {
            Some(row) if row.request_fingerprint == request_fingerprint => {
                self.load(Some(user_id), &row.id).await.map(Some)
            }
            Some(_) => Err(RepositoryError::Conflict(
                "Idempotency-Key was already used with a different order".to_string(),
            )),
            None => Ok(None),
        }
    }
    async fn create(&self, draft: NewOrder) -> Result<CreateResult, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let inserted = sqlx::query_scalar::<_, String>("INSERT INTO mall_orders (id,user_id,idempotency_key,request_fingerprint,status,currency,total_cents,expires_at,node_offer_id,affiliate_creator_id,commission_cents) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (user_id,idempotency_key) DO NOTHING RETURNING id").bind(&draft.order.id).bind(&draft.order.user_id).bind(&draft.idempotency_key).bind(&draft.request_fingerprint).bind(status_name(draft.order.status)?).bind(&draft.order.currency).bind(draft.order.total_cents).bind(parse_timestamp(&draft.order.expires_at)?).bind(&draft.order.node_offer_id).bind(&draft.order.affiliate_creator_id).bind(draft.order.commission_cents).fetch_optional(&mut *tx).await.map_err(database)?;
        let id = if let Some(id) = inserted {
            for line in &draft.order.items {
                sqlx::query("INSERT INTO mall_order_items (order_id,sku_id,product_id,title,quantity,unit_price_cents,currency,line_total_cents) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(&id).bind(&line.sku_id).bind(&line.product_id).bind(&line.title).bind(i64::from(line.quantity)).bind(line.unit_price_cents).bind(&line.currency).bind(line.line_total_cents).execute(&mut *tx).await.map_err(database)?;
            }
            id
        } else {
            let existing = sqlx::query_as::<_, IdempotencyRow>(
                "SELECT id,request_fingerprint FROM mall_orders WHERE user_id=$1 AND idempotency_key=$2",
            )
            .bind(&draft.order.user_id)
            .bind(&draft.idempotency_key)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if existing.request_fingerprint != draft.request_fingerprint {
                return Err(RepositoryError::Conflict(
                    "Idempotency-Key was already used with a different order".to_string(),
                ));
            }
            existing.id
        };
        tx.commit().await.map_err(database)?;
        Ok(CreateResult {
            order: self.load(Some(&draft.order.user_id), &id).await?,
        })
    }
    async fn get(&self, user_id: &str, id: &str) -> Result<pb::Order, RepositoryError> {
        self.load(Some(user_id), id).await
    }
    async fn list(&self, user_id: &str) -> Result<Vec<pb::Order>, RepositoryError> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM mall_orders WHERE user_id=$1 ORDER BY id DESC LIMIT 100",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            values.push(self.load(Some(user_id), &id).await?);
        }
        Ok(values)
    }
    async fn expired_pending(&self, limit: usize) -> Result<Vec<pb::Order>, RepositoryError> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM mall_orders WHERE status='pending_payment' AND expires_at <= now() ORDER BY expires_at,id LIMIT $1",
        )
        .bind(i64::try_from(limit.clamp(1, 1_000)).unwrap_or(1_000))
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        let mut orders = Vec::with_capacity(ids.len());
        for id in ids {
            orders.push(self.load(None, &id).await?);
        }
        Ok(orders)
    }
    async fn claim_payment_reference(
        &self,
        id: &str,
        payment_reference: &str,
    ) -> Result<pb::Order, RepositoryError> {
        let changed = sqlx::query(
            "UPDATE mall_orders SET payment_reference=$2,updated_at=now() WHERE id=$1 AND status='pending_payment' AND expires_at > now() AND (payment_reference IS NULL OR payment_reference=$2)",
        )
        .bind(id)
        .bind(payment_reference)
        .execute(&self.pool)
        .await
        .map_err(payment_reference_error)?
        .rows_affected();
        if changed == 1 {
            return self.load(None, id).await;
        }
        let order = self.load(None, id).await?;
        if order.status == pb::MallOrderStatus::Paid as i32
            && order.payment_reference.as_deref() == Some(payment_reference)
        {
            return Ok(order);
        }
        if order.status == pb::MallOrderStatus::PendingPayment as i32 {
            if OffsetDateTime::parse(&order.expires_at, &Rfc3339)
                .is_ok_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
            {
                return Err(RepositoryError::State(
                    "order payment window has expired".to_string(),
                ));
            }
            return Err(RepositoryError::Conflict(
                "order already has a different payment reference".to_string(),
            ));
        }
        Err(RepositoryError::Failed(format!(
            "order {id} is not pending payment"
        )))
    }
    async fn transition(
        &self,
        id: &str,
        status: i32,
        payment_reference: Option<String>,
    ) -> Result<pb::Order, RepositoryError> {
        let changed = sqlx::query("UPDATE mall_orders SET status=$2,payment_reference=COALESCE($3,payment_reference),updated_at=now() WHERE id=$1 AND status='pending_payment'").bind(id).bind(status_name(status)?).bind(payment_reference).execute(&self.pool).await.map_err(payment_reference_error)?.rows_affected();
        if changed == 0 {
            let order = self.load(None, id).await?;
            if order.status == status {
                return Ok(order);
            }
            return Err(RepositoryError::Failed(format!(
                "order {id} is not pending payment"
            )));
        }
        self.load(None, id).await
    }
}
#[derive(FromRow)]
struct OrderRow {
    id: String,
    user_id: String,
    status: String,
    currency: String,
    total_cents: i64,
    payment_reference: Option<String>,
    expires_at: OffsetDateTime,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    node_offer_id: Option<String>,
    affiliate_creator_id: Option<String>,
    commission_cents: i64,
}
#[derive(FromRow)]
struct IdempotencyRow {
    id: String,
    request_fingerprint: String,
}
#[derive(FromRow)]
struct OrderLineRow {
    sku_id: String,
    product_id: String,
    title: String,
    quantity: i64,
    unit_price_cents: i64,
    currency: String,
    line_total_cents: i64,
}
fn order_from_row(row: OrderRow, items: Vec<OrderLineRow>) -> Result<pb::Order, RepositoryError> {
    Ok(pb::Order {
        id: row.id,
        user_id: row.user_id,
        status: parse_status(&row.status)?,
        currency: row.currency,
        total_cents: row.total_cents,
        items: items
            .into_iter()
            .map(|line| {
                Ok(pb::OrderLine {
                    sku_id: line.sku_id,
                    product_id: line.product_id,
                    title: line.title,
                    quantity: u32::try_from(line.quantity).map_err(|_| {
                        RepositoryError::Failed("invalid stored line quantity".to_string())
                    })?,
                    unit_price_cents: line.unit_price_cents,
                    currency: line.currency,
                    line_total_cents: line.line_total_cents,
                })
            })
            .collect::<Result<_, _>>()?,
        payment_reference: row.payment_reference,
        expires_at: timestamp(row.expires_at),
        created_at: timestamp(row.created_at),
        updated_at: timestamp(row.updated_at),
        node_offer_id: row.node_offer_id,
        affiliate_creator_id: row.affiliate_creator_id,
        commission_cents: row.commission_cents,
    })
}
fn status_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::MallOrderStatus::try_from(value).ok() {
        Some(pb::MallOrderStatus::PendingPayment) => Ok("pending_payment"),
        Some(pb::MallOrderStatus::Paid) => Ok("paid"),
        Some(pb::MallOrderStatus::Cancelled) => Ok("cancelled"),
        Some(pb::MallOrderStatus::Expired) => Ok("expired"),
        None => Err(RepositoryError::Failed("invalid order status".to_string())),
    }
}
fn parse_status(value: &str) -> Result<i32, RepositoryError> {
    match value {
        "pending_payment" => Ok(pb::MallOrderStatus::PendingPayment as i32),
        "paid" => Ok(pb::MallOrderStatus::Paid as i32),
        "cancelled" => Ok(pb::MallOrderStatus::Cancelled as i32),
        "expired" => Ok(pb::MallOrderStatus::Expired as i32),
        _ => Err(RepositoryError::Failed(format!(
            "unknown order status {value}"
        ))),
    }
}
fn parse_timestamp(value: &str) -> Result<OffsetDateTime, RepositoryError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| RepositoryError::Failed(error.to_string()))
}
fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
fn database(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Failed(error.to_string())
}
fn payment_reference_error(error: sqlx::Error) -> RepositoryError {
    if let sqlx::Error::Database(value) = &error
        && value.code().as_deref() == Some("23505")
    {
        return RepositoryError::Conflict(
            "payment reference belongs to a different order".to_string(),
        );
    }
    database(error)
}

#[cfg(test)]
mod tests {
    use super::{MemoryOrderRepository, NewOrder, OrderRepository, RepositoryError};
    use crate::api::pb;

    fn draft(id: &str) -> pb::Order {
        pb::Order {
            id: id.to_string(),
            user_id: "user-1".to_string(),
            status: pb::MallOrderStatus::PendingPayment as i32,
            currency: "CNY".to_string(),
            total_cents: 100,
            items: Vec::new(),
            payment_reference: None,
            expires_at: "2099-08-16T01:00:00Z".to_string(),
            created_at: "2026-08-16T00:00:00Z".to_string(),
            updated_at: "2026-08-16T00:00:00Z".to_string(),
            node_offer_id: None,
            affiliate_creator_id: None,
            commission_cents: 0,
        }
    }

    #[tokio::test]
    async fn idempotency_key_rejects_a_different_order_payload() {
        let repository = MemoryOrderRepository::default();
        repository
            .create(NewOrder {
                order: draft("order-1"),
                idempotency_key: "key-1".to_string(),
                request_fingerprint: "sku-1:1".to_string(),
            })
            .await
            .expect("initial order should be created");
        let retry = repository
            .find_idempotent("user-1", "key-1", "sku-1:1")
            .await
            .expect("matching payload should replay the order");
        assert_eq!(retry.expect("order should exist").id, "order-1");
        let error = repository
            .find_idempotent("user-1", "key-1", "sku-2:1")
            .await
            .expect_err("different payload must not reuse the order");
        assert!(matches!(error, RepositoryError::Conflict(_)));
    }

    #[tokio::test]
    async fn payment_reference_cannot_be_claimed_by_two_orders() {
        let repository = MemoryOrderRepository::default();
        repository
            .create(NewOrder {
                order: draft("order-1"),
                idempotency_key: "key-1".to_string(),
                request_fingerprint: "sku-1:1".to_string(),
            })
            .await
            .expect("first order should be created");
        repository
            .create(NewOrder {
                order: draft("order-2"),
                idempotency_key: "key-2".to_string(),
                request_fingerprint: "sku-2:1".to_string(),
            })
            .await
            .expect("second order should be created");
        repository
            .claim_payment_reference("order-1", "payment-1")
            .await
            .expect("first claim should succeed");
        let error = repository
            .claim_payment_reference("order-2", "payment-1")
            .await
            .expect_err("payment reference reuse must fail");
        assert!(matches!(error, RepositoryError::Conflict(_)));
    }

    #[tokio::test]
    async fn payment_reference_cannot_be_claimed_after_the_order_expires() {
        let repository = MemoryOrderRepository::default();
        let mut order = draft("order-1");
        order.expires_at = "2000-01-01T00:00:00Z".to_string();
        repository
            .create(NewOrder {
                order,
                idempotency_key: "key-1".to_string(),
                request_fingerprint: "sku-1:1".to_string(),
            })
            .await
            .expect("order should be created");
        let error = repository
            .claim_payment_reference("order-1", "payment-1")
            .await
            .expect_err("expired orders cannot start payment");
        assert!(matches!(error, RepositoryError::State(_)));
    }
}
