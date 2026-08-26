use super::*;
use sqlx::PgPool;

#[derive(sqlx::FromRow)]
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
    node_offer_id: String,
    affiliate_creator_id: String,
    commission_cents: i64,
    merchant_id: String,
    fulfillment_status: String,
    tracking_number: String,
}

#[derive(sqlx::FromRow)]
struct IdempotencyRow {
    id: String,
    request_fingerprint: String,
}

#[derive(sqlx::FromRow)]
struct OrderLineRow {
    sku_id: String,
    product_id: String,
    title: String,
    quantity: i64,
    unit_price_cents: i64,
    currency: String,
    line_total_cents: i64,
}

#[derive(sqlx::FromRow)]
struct SettlementRow {
    id: String,
    order_id: String,
    merchant_id: String,
    creator_id: String,
    amount_cents: i64,
    status: String,
    eligible_at: OffsetDateTime,
    settled_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
}

fn order_from_row(row: OrderRow, items: Vec<OrderLineRow>) -> Result<pb::Order, DaoError> {
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
                        DaoError::Failed("invalid stored line quantity".to_string())
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
        merchant_id: row.merchant_id,
        fulfillment_status: parse_fulfillment_status(&row.fulfillment_status)?,
        tracking_number: row.tracking_number,
    })
}

fn settlement_from_row(row: SettlementRow) -> Result<pb::AffiliateSettlement, DaoError> {
    Ok(pb::AffiliateSettlement {
        id: row.id,
        order_id: row.order_id,
        merchant_id: row.merchant_id,
        creator_id: row.creator_id,
        amount_cents: row.amount_cents,
        status: parse_settlement_status(&row.status)?,
        eligible_at: timestamp(row.eligible_at),
        settled_at: row.settled_at.map(timestamp),
        created_at: timestamp(row.created_at),
    })
}

#[derive(Clone)]
pub(crate) struct PostgresOrderDao {
    pool: PgPool,
}

impl PostgresOrderDao {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn load(&self, user_id: Option<&str>, id: &str) -> Result<pb::Order, DaoError> {
        let row = if let Some(user_id) = user_id {
            sqlx::query_as::<_, OrderRow>("SELECT id,user_id,status,currency,total_cents,payment_reference,expires_at,created_at,updated_at,node_offer_id,affiliate_creator_id,commission_cents,merchant_id,fulfillment_status,tracking_number FROM mall_orders WHERE id=$1 AND user_id=$2").bind(id).bind(user_id).fetch_optional(&self.pool).await.map_err(database)?
        } else {
            sqlx::query_as::<_, OrderRow>("SELECT id,user_id,status,currency,total_cents,payment_reference,expires_at,created_at,updated_at,node_offer_id,affiliate_creator_id,commission_cents,merchant_id,fulfillment_status,tracking_number FROM mall_orders WHERE id=$1").bind(id).fetch_optional(&self.pool).await.map_err(database)?
        };
        let row = row.ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        let items = sqlx::query_as::<_, OrderLineRow>("SELECT sku_id,product_id,title,quantity,unit_price_cents,currency,line_total_cents FROM mall_order_items WHERE order_id=$1 ORDER BY sku_id").bind(id).fetch_all(&self.pool).await.map_err(database)?;
        order_from_row(row, items)
    }
}

#[async_trait]
impl OrderDao for PostgresOrderDao {
    async fn find_idempotent(
        &self,
        user_id: &str,
        key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Order>, DaoError> {
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
            Some(_) => Err(DaoError::Conflict(
                "Idempotency-Key was already used with a different order".to_string(),
            )),
            None => Ok(None),
        }
    }
    async fn create(&self, draft: NewOrder) -> Result<CreateResult, DaoError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let inserted = sqlx::query_scalar::<_, String>("INSERT INTO mall_orders (id,user_id,idempotency_key,request_fingerprint,status,currency,total_cents,expires_at,node_offer_id,affiliate_creator_id,commission_cents,merchant_id,fulfillment_status,tracking_number) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (user_id,idempotency_key) DO NOTHING RETURNING id").bind(&draft.order.id).bind(&draft.order.user_id).bind(&draft.idempotency_key).bind(&draft.request_fingerprint).bind(status_name(draft.order.status)?).bind(&draft.order.currency).bind(draft.order.total_cents).bind(parse_timestamp(&draft.order.expires_at)?).bind(&draft.order.node_offer_id).bind(&draft.order.affiliate_creator_id).bind(draft.order.commission_cents).bind(&draft.order.merchant_id).bind(fulfillment_name(draft.order.fulfillment_status)?).bind(&draft.order.tracking_number).fetch_optional(&mut *tx).await.map_err(database)?;
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
                return Err(DaoError::Conflict(
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
    async fn get(&self, user_id: &str, id: &str) -> Result<pb::Order, DaoError> {
        self.load(Some(user_id), id).await
    }
    async fn list(&self, user_id: &str) -> Result<Vec<pb::Order>, DaoError> {
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
    async fn expired_pending(&self, limit: usize) -> Result<Vec<pb::Order>, DaoError> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM mall_orders WHERE status IN ('pending_payment','payment_processing') AND expires_at <= now() ORDER BY expires_at,id LIMIT $1",
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
    async fn begin_payment(
        &self,
        id: &str,
        payment_reference: &str,
    ) -> Result<pb::Order, DaoError> {
        let changed = sqlx::query(
            "UPDATE mall_orders SET status='payment_processing',payment_reference=$2,updated_at=now() WHERE id=$1 AND status='pending_payment' AND expires_at > now() AND (payment_reference IS NULL OR payment_reference=$2)",
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
        if order.status == pb::MallOrderStatus::PaymentProcessing as i32
            && order.payment_reference.as_deref() == Some(payment_reference)
        {
            if OffsetDateTime::parse(&order.expires_at, &Rfc3339)
                .is_ok_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
            {
                return Err(DaoError::State(
                    "order payment window has expired".to_string(),
                ));
            }
            return Ok(order);
        }
        if order.status == pb::MallOrderStatus::PendingPayment as i32 {
            if OffsetDateTime::parse(&order.expires_at, &Rfc3339)
                .is_ok_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
            {
                return Err(DaoError::State(
                    "order payment window has expired".to_string(),
                ));
            }
            return Err(DaoError::Conflict(
                "order already has a different payment reference".to_string(),
            ));
        }
        Err(DaoError::Failed(format!(
            "order {id} is not in a transitionable payment state"
        )))
    }
    async fn transition(
        &self,
        id: &str,
        status: i32,
        payment_reference: Option<String>,
    ) -> Result<pb::Order, DaoError> {
        let source_clause = match status {
            value if value == pb::MallOrderStatus::Paid as i32 => "status='payment_processing'",
            value if value == pb::MallOrderStatus::Cancelled as i32 => "status='pending_payment'",
            value if value == pb::MallOrderStatus::Expired as i32 => {
                "status IN ('pending_payment','payment_processing')"
            }
            _ => "FALSE",
        };
        let statement = format!(
            "UPDATE mall_orders SET status=$2,payment_reference=COALESCE($3,payment_reference),updated_at=now() WHERE id=$1 AND {source_clause}"
        );
        let changed = sqlx::query(&statement)
            .bind(id)
            .bind(status_name(status)?)
            .bind(payment_reference)
            .execute(&self.pool)
            .await
            .map_err(payment_reference_error)?
            .rows_affected();
        if changed == 0 {
            let order = self.load(None, id).await?;
            if order.status == status {
                return Ok(order);
            }
            return Err(DaoError::Failed(format!(
                "order {id} is not in a transitionable payment state"
            )));
        }
        self.load(None, id).await
    }
    async fn merchant_orders(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::Order>, DaoError> {
        let limit = i64::try_from(limit.clamp(1, 100)).unwrap_or(100);
        let status = status.map(status_name).transpose()?;
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM mall_orders WHERE merchant_id=$1 AND ($2::text IS NULL OR status=$2) AND ($3='' OR id < $3) ORDER BY id DESC LIMIT $4",
        )
        .bind(merchant_id)
        .bind(status)
        .bind(cursor.unwrap_or_default())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            values.push(self.load(None, &id).await?);
        }
        Ok(values)
    }
    async fn update_fulfillment(
        &self,
        merchant_id: &str,
        order_id: &str,
        status: i32,
        tracking_number: &str,
    ) -> Result<pb::Order, DaoError> {
        let status_name = fulfillment_name(status)?;
        if status == pb::FulfillmentStatus::Shipped as i32 && tracking_number.trim().is_empty() {
            return Err(DaoError::State(
                "tracking number is required when shipping".to_string(),
            ));
        }
        let current = self.load(None, order_id).await?;
        if current.merchant_id != merchant_id {
            return Err(DaoError::NotFound(order_id.to_string()));
        }
        if current.status != pb::MallOrderStatus::Paid as i32 {
            return Err(DaoError::State(
                "only paid orders can be fulfilled".to_string(),
            ));
        }
        validate_fulfillment_transition(current.fulfillment_status, status)?;
        let changed = sqlx::query(
            "UPDATE mall_orders SET fulfillment_status=$3,tracking_number=CASE WHEN $4='' THEN tracking_number ELSE $4 END,updated_at=now() WHERE id=$1 AND merchant_id=$2 AND fulfillment_status=$5",
        )
        .bind(order_id)
        .bind(merchant_id)
        .bind(status_name)
        .bind(tracking_number)
        .bind(fulfillment_name(current.fulfillment_status)? )
        .execute(&self.pool)
        .await
        .map_err(database)?
        .rows_affected();
        if changed == 0 {
            return self.load(None, order_id).await;
        }
        self.load(None, order_id).await
    }
    async fn ensure_settlement(&self, order: &pb::Order) -> Result<(), DaoError> {
        sqlx::query(
            "INSERT INTO mall_affiliate_settlements (id,order_id,merchant_id,creator_id,amount_cents,status,eligible_at) VALUES ($1,$2,$3,$4,$5,'eligible',now()) ON CONFLICT (order_id) DO NOTHING",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(&order.id)
        .bind(&order.merchant_id)
        .bind(&order.affiliate_creator_id)
        .bind(order.commission_cents)
        .execute(&self.pool)
        .await
        .map_err(database)?;
        Ok(())
    }
    async fn settlements(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::AffiliateSettlement>, DaoError> {
        let status = status.map(settlement_status_name).transpose()?;
        let rows = sqlx::query_as::<_, SettlementRow>(
            "SELECT id,order_id,merchant_id,creator_id,amount_cents,status,eligible_at,settled_at,created_at FROM mall_affiliate_settlements WHERE merchant_id=$1 AND ($2::text IS NULL OR status=$2) AND ($3='' OR id < $3) ORDER BY id DESC LIMIT $4",
        )
        .bind(merchant_id)
        .bind(status)
        .bind(cursor.unwrap_or_default())
        .bind(i64::try_from(limit.clamp(1, 100)).unwrap_or(100))
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        rows.into_iter().map(settlement_from_row).collect()
    }
    async fn settle_affiliate(
        &self,
        merchant_id: &str,
        settlement_id: &str,
    ) -> Result<pb::AffiliateSettlement, DaoError> {
        let changed = sqlx::query("UPDATE mall_affiliate_settlements SET status='settled',settled_at=now(),updated_at=now() WHERE id=$1 AND merchant_id=$2 AND status='eligible'")
            .bind(settlement_id)
            .bind(merchant_id)
            .execute(&self.pool)
            .await
            .map_err(database)?
            .rows_affected();
        let row = sqlx::query_as::<_, SettlementRow>("SELECT id,order_id,merchant_id,creator_id,amount_cents,status,eligible_at,settled_at,created_at FROM mall_affiliate_settlements WHERE id=$1 AND merchant_id=$2")
            .bind(settlement_id)
            .bind(merchant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(database)?
            .ok_or_else(|| DaoError::NotFound(settlement_id.to_string()))?;
        if changed == 0 && row.status != "settled" {
            return Err(DaoError::State("settlement is not eligible".to_string()));
        }
        settlement_from_row(row)
    }
}
