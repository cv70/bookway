use super::*;

/// Test-side mirror of the Postgres `purchase_event_outbox` row payload. A
/// real purchase relay only exists against Postgres storage, so this queue
/// (and its capture in `transition`) is compiled for tests alone; production
/// attribution goes through the SQL lane documented in mall-order's README.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct PurchaseOutboxEntry {
    pub(crate) order_id: String,
    pub(crate) user_id: String,
    pub(crate) node_offer_id: String,
}

#[derive(Default)]
pub(crate) struct MemoryOrderDao {
    orders: RwLock<HashMap<String, MemoryOrder>>,
    idempotency: RwLock<HashMap<(String, String), String>>,
    payment_references: RwLock<HashMap<String, String>>,
    settlements: RwLock<HashMap<String, pb::AffiliateSettlement>>,
    #[cfg(test)]
    purchase_queue: RwLock<Vec<PurchaseOutboxEntry>>,
}

#[cfg(test)]
impl MemoryOrderDao {
    pub(crate) async fn purchase_queue(&self) -> Vec<PurchaseOutboxEntry> {
        self.purchase_queue.read().await.clone()
    }
}

// Both binaries call this at the same point the Postgres dao enqueues the
// outbox row. Only test builds retain the rows: the real delivery lane
// consumes the SQL table directly, so a plain memory-mode process has no
// reader for them.
impl MemoryOrderDao {
    async fn mirror_purchase_outbox(&self, order: &pb::Order) {
        let _ = order;
        #[cfg(test)]
        {
            if order.status != pb::MallOrderStatus::Paid as i32 || order.node_offer_id.is_empty()
            {
                return;
            }
            self.purchase_queue.write().await.push(PurchaseOutboxEntry {
                order_id: order.id.clone(),
                user_id: order.user_id.clone(),
                node_offer_id: order.node_offer_id.clone(),
            });
        }
    }
}

#[async_trait]
impl OrderDao for MemoryOrderDao {
    async fn find_idempotent(
        &self,
        user_id: &str,
        key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Order>, DaoError> {
        let index = self.idempotency.read().await;
        let Some(id) = index.get(&(user_id.to_string(), key.to_string())) else {
            return Ok(None);
        };
        let order = self.orders.read().await.get(id).cloned();
        match order {
            Some(order) if order.request_fingerprint == request_fingerprint => {
                Ok(Some(order.order))
            }
            Some(_) => Err(DaoError::Conflict(
                "Idempotency-Key was already used with a different order".to_string(),
            )),
            None => Err(DaoError::Failed("missing idempotency target".to_string())),
        }
    }
    async fn create(&self, draft: NewOrder) -> Result<CreateResult, DaoError> {
        let mut index = self.idempotency.write().await;
        if let Some(id) = index.get(&(draft.order.user_id.clone(), draft.idempotency_key.clone())) {
            let stored = self
                .orders
                .read()
                .await
                .get(id)
                .ok_or_else(|| DaoError::Failed("missing idempotency target".to_string()))?
                .clone();
            if stored.request_fingerprint != draft.request_fingerprint {
                return Err(DaoError::Conflict(
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
    async fn get(&self, user_id: &str, id: &str) -> Result<pb::Order, DaoError> {
        self.orders
            .read()
            .await
            .get(id)
            .filter(|order| order.order.user_id == user_id)
            .map(|order| order.order.clone())
            .ok_or_else(|| DaoError::NotFound(id.to_string()))
    }
    async fn list(&self, user_id: &str) -> Result<Vec<pb::Order>, DaoError> {
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
    async fn expired_pending(&self, limit: usize) -> Result<Vec<pb::Order>, DaoError> {
        let now = OffsetDateTime::now_utc();
        let mut values = self
            .orders
            .read()
            .await
            .values()
            .filter(|order| {
                matches!(
                    order.order.status,
                    value if value == pb::MallOrderStatus::PendingPayment as i32
                        || value == pb::MallOrderStatus::PaymentProcessing as i32
                ) && OffsetDateTime::parse(&order.order.expires_at, &Rfc3339)
                    .is_ok_and(|value| value <= now)
            })
            .map(|order| order.order.clone())
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.expires_at.cmp(&right.expires_at));
        values.truncate(limit);
        Ok(values)
    }
    async fn begin_payment(
        &self,
        id: &str,
        payment_reference: &str,
    ) -> Result<pb::Order, DaoError> {
        let mut orders = self.orders.write().await;
        let order = orders
            .get_mut(id)
            .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        if order.order.status == pb::MallOrderStatus::Paid as i32 {
            if order.order.payment_reference.as_deref() == Some(payment_reference) {
                return Ok(order.order.clone());
            }
            return Err(DaoError::Conflict(
                "payment reference belongs to a different order".to_string(),
            ));
        }
        if order.order.status == pb::MallOrderStatus::PaymentProcessing as i32 {
            if order.order.payment_reference.as_deref() == Some(payment_reference) {
                if OffsetDateTime::parse(&order.order.expires_at, &Rfc3339)
                    .is_ok_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
                {
                    return Err(DaoError::State(
                        "order payment window has expired".to_string(),
                    ));
                }
                return Ok(order.order.clone());
            }
            return Err(DaoError::Conflict(
                "order is already processing a different payment".to_string(),
            ));
        }
        if order.order.status != pb::MallOrderStatus::PendingPayment as i32 {
            return Err(DaoError::Failed(format!(
                "order {id} is not pending payment"
            )));
        }
        if OffsetDateTime::parse(&order.order.expires_at, &Rfc3339)
            .is_ok_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
        {
            return Err(DaoError::State(
                "order payment window has expired".to_string(),
            ));
        }
        if let Some(existing) = &order.order.payment_reference {
            if existing != payment_reference {
                return Err(DaoError::Conflict(
                    "order already has a different payment reference".to_string(),
                ));
            }
            return Ok(order.order.clone());
        }
        let mut references = self.payment_references.write().await;
        if let Some(owner) = references.get(payment_reference)
            && owner != id
        {
            return Err(DaoError::Conflict(
                "payment reference belongs to a different order".to_string(),
            ));
        }
        references.insert(payment_reference.to_string(), id.to_string());
        order.order.payment_reference = Some(payment_reference.to_string());
        order.order.status = pb::MallOrderStatus::PaymentProcessing as i32;
        order.order.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(order.order.clone())
    }
    async fn transition(
        &self,
        id: &str,
        status: i32,
        payment_reference: Option<String>,
    ) -> Result<pb::Order, DaoError> {
        let mut orders = self.orders.write().await;
        let order = orders
            .get_mut(id)
            .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        if order.order.status == status {
            if status == pb::MallOrderStatus::Paid as i32
                && payment_reference.is_some()
                && order.order.payment_reference != payment_reference
            {
                return Err(DaoError::Conflict(
                    "payment reference belongs to a different payment".to_string(),
                ));
            }
            return Ok(order.order.clone());
        }
        let allowed_source = (status == pb::MallOrderStatus::Paid as i32
            && order.order.status == pb::MallOrderStatus::PaymentProcessing as i32)
            || (status == pb::MallOrderStatus::Cancelled as i32
                && order.order.status == pb::MallOrderStatus::PendingPayment as i32)
            || (status == pb::MallOrderStatus::Expired as i32
                && (order.order.status == pb::MallOrderStatus::PendingPayment as i32
                    || order.order.status == pb::MallOrderStatus::PaymentProcessing as i32));
        if !allowed_source {
            return Err(DaoError::Failed(format!(
                "order {id} is not in a transitionable payment state"
            )));
        }
        if status == pb::MallOrderStatus::Paid as i32 {
            let reference = payment_reference
                .or_else(|| order.order.payment_reference.clone())
                .ok_or_else(|| {
                    DaoError::Failed("paid orders require a payment reference".to_string())
                })?;
            let mut references = self.payment_references.write().await;
            if let Some(owner) = references.get(&reference)
                && owner != id
            {
                return Err(DaoError::Conflict(
                    "payment reference belongs to a different order".to_string(),
                ));
            }
            references.insert(reference.clone(), id.to_string());
            order.order.payment_reference = Some(reference);
        } else if payment_reference.is_some() {
            return Err(DaoError::Failed(
                "only paid orders can carry a payment reference".to_string(),
            ));
        }
        order.order.status = status;
        order.order.updated_at = timestamp(OffsetDateTime::now_utc());
        // Mirrors the Postgres transition: a paid contextual order enters the
        // attribution outbox exactly once, in the same step that flips the
        // state. Unattributed carts never enqueue.
        self.mirror_purchase_outbox(&order.order).await;
        Ok(order.order.clone())
    }
    async fn merchant_orders(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::Order>, DaoError> {
        let cursor = cursor.unwrap_or_default();
        let mut values = self
            .orders
            .read()
            .await
            .values()
            .map(|stored| stored.order.clone())
            .filter(|order| order.merchant_id == merchant_id)
            .filter(|order| status.is_none_or(|value| order.status == value))
            .filter(|order| cursor.is_empty() || order.id.as_str() < cursor)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| right.id.cmp(&left.id));
        values.truncate(limit.clamp(1, 100));
        Ok(values)
    }
    async fn update_fulfillment(
        &self,
        merchant_id: &str,
        order_id: &str,
        status: i32,
        tracking_number: &str,
    ) -> Result<pb::Order, DaoError> {
        let mut orders = self.orders.write().await;
        let order = orders
            .get_mut(order_id)
            .ok_or_else(|| DaoError::NotFound(order_id.to_string()))?;
        if order.order.merchant_id != merchant_id {
            return Err(DaoError::NotFound(order_id.to_string()));
        }
        if order.order.status != pb::MallOrderStatus::Paid as i32 {
            return Err(DaoError::State(
                "only paid orders can be fulfilled".to_string(),
            ));
        }
        validate_fulfillment_transition(order.order.fulfillment_status, status)?;
        if status == pb::FulfillmentStatus::Shipped as i32 && tracking_number.trim().is_empty() {
            return Err(DaoError::State(
                "tracking number is required when shipping".to_string(),
            ));
        }
        order.order.fulfillment_status = status;
        if !tracking_number.trim().is_empty() {
            order.order.tracking_number = tracking_number.to_string();
        }
        order.order.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(order.order.clone())
    }
    async fn ensure_settlement(&self, order: &pb::Order) -> Result<(), DaoError> {
        let mut settlements = self.settlements.write().await;
        settlements
            .entry(order.id.clone())
            .or_insert_with(|| pb::AffiliateSettlement {
                id: uuid::Uuid::now_v7().to_string(),
                order_id: order.id.clone(),
                merchant_id: order.merchant_id.clone(),
                creator_id: order.affiliate_creator_id.clone(),
                amount_cents: order.commission_cents,
                status: pb::AffiliateSettlementStatus::Eligible as i32,
                eligible_at: timestamp(OffsetDateTime::now_utc()),
                settled_at: None,
                created_at: timestamp(OffsetDateTime::now_utc()),
            });
        Ok(())
    }
    async fn settlements(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::AffiliateSettlement>, DaoError> {
        let cursor = cursor.unwrap_or_default();
        let mut values = self
            .settlements
            .read()
            .await
            .values()
            .filter(|item| item.merchant_id == merchant_id)
            .filter(|item| status.is_none_or(|value| item.status == value))
            .filter(|item| cursor.is_empty() || item.id.as_str() < cursor)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|left, right| right.id.cmp(&left.id));
        values.truncate(limit.clamp(1, 100));
        Ok(values)
    }
    async fn settle_affiliate(
        &self,
        merchant_id: &str,
        settlement_id: &str,
    ) -> Result<pb::AffiliateSettlement, DaoError> {
        let mut settlements = self.settlements.write().await;
        let item = settlements
            .values_mut()
            .find(|item| item.id == settlement_id && item.merchant_id == merchant_id)
            .ok_or_else(|| DaoError::NotFound(settlement_id.to_string()))?;
        if item.status == pb::AffiliateSettlementStatus::Settled as i32 {
            return Ok(item.clone());
        }
        if item.status != pb::AffiliateSettlementStatus::Eligible as i32 {
            return Err(DaoError::State("settlement is not eligible".to_string()));
        }
        item.status = pb::AffiliateSettlementStatus::Settled as i32;
        item.settled_at = Some(timestamp(OffsetDateTime::now_utc()));
        Ok(item.clone())
    }
    async fn reverse_affiliate(&self, order_id: &str) -> Result<pb::AffiliateSettlement, DaoError> {
        let mut settlements = self.settlements.write().await;
        let item = settlements
            .get_mut(order_id)
            .ok_or_else(|| DaoError::NotFound(order_id.to_string()))?;
        match pb::AffiliateSettlementStatus::try_from(item.status).ok() {
            Some(pb::AffiliateSettlementStatus::Eligible) => {
                item.status = pb::AffiliateSettlementStatus::Reversed as i32;
                item.settled_at = None;
                Ok(item.clone())
            }
            Some(pb::AffiliateSettlementStatus::Reversed) => Ok(item.clone()),
            _ => Err(DaoError::State("settlement is not reversible".to_string())),
        }
    }
}
