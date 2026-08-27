use crate::api::pb;
use crate::{
    Config,
    datasource::{CreateResult, DaoError, MemoryOrderDao, NewOrder, OrderDao, PostgresOrderDao},
};
use bookway_mall_api::pb as mall_pb;
use bookway_mall_inventory_api::pb as inventory_pb;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tonic::transport::Channel;
use uuid::Uuid;

const MAX_IDENTIFIER_LENGTH: usize = 160;
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 220;
const MAX_PAYMENT_REFERENCE_LENGTH: usize = 220;
const MAX_TRACKING_NUMBER_LENGTH: usize = 220;

#[derive(Debug, Error)]
pub(crate) enum OrderError {
    #[error("{0}")]
    Validation(String),
    #[error("order {0} was not found")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    State(String),
    #[error("{0} request failed: {1}")]
    Upstream(&'static str, String),
    #[error("order operation failed: {0}")]
    Repository(String),
}

#[derive(Clone)]
pub struct Domain {
    config: Config,
    dao: Arc<dyn OrderDao>,
    mall: mall_pb::mall_client::MallClient<Channel>,
    inventory: inventory_pb::mall_inventory_client::MallInventoryClient<Channel>,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let dao: Arc<dyn OrderDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryOrderDao::default()),
            bookway_data::StorageMode::Postgres => {
                Arc::new(PostgresOrderDao::new(bookway_data::postgres_pool().await?))
            }
        };
        let mall = mall_pb::mall_client::MallClient::new(
            bookway_runtime::grpc_channel(&config.mall_url).await?,
        );
        let inventory = inventory_pb::mall_inventory_client::MallInventoryClient::new(
            bookway_runtime::grpc_channel(&config.inventory_url).await?,
        );
        Ok(Self {
            config,
            dao,
            mall,
            inventory,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) async fn create(&self, request: pb::CreateRequest) -> Result<pb::Order, OrderError> {
        let mut request = request;
        request.user_id = request.user_id.trim().to_string();
        request.idempotency_key = request.idempotency_key.trim().to_string();
        request.node_offer_id = request.node_offer_id.trim().to_string();
        for item in &mut request.items {
            item.sku_id = item.sku_id.trim().to_string();
        }
        if invalid_identifier(&request.user_id)
            || request.idempotency_key.is_empty()
            || request.idempotency_key.chars().count() > MAX_IDEMPOTENCY_KEY_LENGTH
        {
            return Err(OrderError::Validation(
                "user_id and Idempotency-Key are required".to_string(),
            ));
        }
        validate_items(&request.items)?;
        let request_fingerprint = request_fingerprint(&request);
        if let Some(order) = self
            .dao
            .find_idempotent(
                &request.user_id,
                &request.idempotency_key,
                &request_fingerprint,
            )
            .await
            .map_err(repo_error)?
        {
            return self.resume_pending(order).await;
        }

        let sku_ids = request
            .items
            .iter()
            .map(|item| item.sku_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let catalog = self.catalog_skus(sku_ids.clone()).await?;
        let sku_map = catalog
            .into_iter()
            .map(|sku| (sku.id.clone(), sku))
            .collect::<BTreeMap<_, _>>();
        if sku_map.len() != sku_ids.len() {
            return Err(OrderError::State(
                "one or more SKUs are no longer saleable".to_string(),
            ));
        }
        if invalid_identifier(&request.node_offer_id) {
            return Err(OrderError::Validation(
                "node offer id is required for contextual checkout".to_string(),
            ));
        }
        let node_offer = self.checkout_node_offer(&request.node_offer_id).await?;
        let item = contextual_order_item(&request.items, &node_offer)?;
        let Some(sku) = sku_map.get(&item.sku_id) else {
            return Err(OrderError::State(
                "node offer SKU is unavailable".to_string(),
            ));
        };
        if node_offer.product_id != sku.product_id {
            return Err(OrderError::Conflict(
                "node offer product does not match the SKU".to_string(),
            ));
        }
        if node_offer.commission_bps > 3_000
            || node_offer.merchant_id.trim().is_empty()
            || node_offer.creator_id.trim().is_empty()
            || node_offer.scene_equipment.trim().is_empty()
        {
            return Err(OrderError::State(
                "node offer contextual metadata is invalid".to_string(),
            ));
        }
        let order = new_order(
            &request.user_id,
            &request.items,
            sku_map,
            self.config.payment_ttl_seconds,
            &node_offer,
        )?;
        let created = self
            .dao
            .create(NewOrder {
                order,
                idempotency_key: request.idempotency_key,
                request_fingerprint,
            })
            .await
            .map_err(repo_error)?;
        self.resume_created(created).await
    }

    pub(crate) async fn list(
        &self,
        mut request: pb::UserRequest,
    ) -> Result<pb::OrderListResponse, OrderError> {
        request.user_id = request.user_id.trim().to_string();
        if invalid_identifier(&request.user_id) {
            return Err(OrderError::Validation("user id is required".to_string()));
        }
        let values = self.dao.list(&request.user_id).await.map_err(repo_error)?;
        let mut items = Vec::with_capacity(values.len());
        for value in values {
            items.push(self.expire_if_needed(value).await?);
        }
        Ok(pb::OrderListResponse { items })
    }

    pub(crate) async fn get(&self, request: pb::OrderRequest) -> Result<pb::Order, OrderError> {
        let mut request = request;
        request.user_id = request.user_id.trim().to_string();
        request.order_id = request.order_id.trim().to_string();
        validate_order_identity(&request.user_id, &request.order_id)?;
        self.get_by_id(&request.user_id, &request.order_id).await
    }

    pub(crate) async fn pay(&self, request: pb::PayRequest) -> Result<pb::Order, OrderError> {
        let mut request = request;
        request.user_id = request.user_id.trim().to_string();
        request.order_id = request.order_id.trim().to_string();
        request.payment_reference = request.payment_reference.trim().to_string();
        validate_order_identity(&request.user_id, &request.order_id)?;
        if request.payment_reference.is_empty()
            || request.payment_reference.chars().count() > MAX_PAYMENT_REFERENCE_LENGTH
        {
            return Err(OrderError::Validation(
                "payment reference is required".to_string(),
            ));
        }
        let order = self.get_by_id(&request.user_id, &request.order_id).await?;
        if order.status == pb::MallOrderStatus::Paid as i32 {
            if order.payment_reference.as_deref() != Some(&request.payment_reference) {
                return Err(OrderError::Conflict(
                    "payment reference belongs to a different payment".to_string(),
                ));
            }
            self.dao
                .ensure_settlement(&order)
                .await
                .map_err(repo_error)?;
            return Ok(order);
        }
        if order.status == pb::MallOrderStatus::PaymentProcessing as i32 {
            if order.payment_reference.as_deref() != Some(&request.payment_reference) {
                return Err(OrderError::Conflict(
                    "order is already processing a different payment".to_string(),
                ));
            }
            // The payment-processing state is durable across a process crash.
            // Once inventory has committed, that fact wins over the order TTL:
            // expiry reconciliation deliberately leaves this state intact so a
            // retry can finish the payment transition. A still-reserved
            // reservation is subject to the normal expiry race and will either
            // commit here or be released by the inventory service.
            let reservation = self.confirm_reservation(&request.order_id).await?;
            if reservation.status != "committed" {
                return Err(OrderError::State(format!(
                    "reservation {} did not commit during payment retry",
                    request.order_id
                )));
            }
            let paid = self
                .dao
                .transition(&request.order_id, pb::MallOrderStatus::Paid as i32, None)
                .await
                .map_err(repo_error)?;
            self.dao
                .ensure_settlement(&paid)
                .await
                .map_err(repo_error)?;
            return Ok(paid);
        }
        if order.status != pb::MallOrderStatus::PendingPayment as i32 {
            return Err(OrderError::State(format!(
                "order {} cannot be paid from its current state",
                request.order_id
            )));
        }
        self.dao
            .begin_payment(&request.order_id, &request.payment_reference)
            .await
            .map_err(repo_error)?;
        let reservation = self.confirm_reservation(&request.order_id).await?;
        if reservation.status != "committed" {
            return Err(OrderError::State(format!(
                "reservation {} did not commit during payment",
                request.order_id
            )));
        }
        let paid = self
            .dao
            .transition(&request.order_id, pb::MallOrderStatus::Paid as i32, None)
            .await
            .map_err(repo_error)?;
        self.dao
            .ensure_settlement(&paid)
            .await
            .map_err(repo_error)?;
        Ok(paid)
    }

    pub(crate) async fn merchant_orders(
        &self,
        mut request: pb::MerchantOrderRequest,
    ) -> Result<pb::MerchantOrderListResponse, OrderError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.cursor = request.cursor.take().map(|value| value.trim().to_string());
        validate_merchant_id(&request.merchant_id)?;
        validate_cursor(request.cursor.as_deref())?;
        let limit = usize::try_from(request.limit.unwrap_or(50).clamp(1, 100)).unwrap_or(100);
        let items = self
            .dao
            .merchant_orders(
                &request.merchant_id,
                request.status,
                request.cursor.as_deref(),
                limit,
            )
            .await
            .map_err(repo_error)?;
        let next_cursor = items
            .last()
            .map(|item| item.id.clone())
            .filter(|_| items.len() == limit);
        Ok(pb::MerchantOrderListResponse { items, next_cursor })
    }

    pub(crate) async fn update_fulfillment(
        &self,
        mut request: pb::UpdateFulfillmentRequest,
    ) -> Result<pb::Order, OrderError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.order_id = request.order_id.trim().to_string();
        request.tracking_number = request.tracking_number.trim().to_string();
        validate_merchant_id(&request.merchant_id)?;
        if invalid_identifier(&request.order_id) {
            return Err(OrderError::Validation("order id is required".to_string()));
        }
        if request.tracking_number.chars().count() > MAX_TRACKING_NUMBER_LENGTH {
            return Err(OrderError::Validation(
                "tracking number is too long".to_string(),
            ));
        }
        if pb::FulfillmentStatus::try_from(request.status).is_err() {
            return Err(OrderError::Validation(
                "invalid fulfillment status".to_string(),
            ));
        }
        self.dao
            .update_fulfillment(
                &request.merchant_id,
                &request.order_id,
                request.status,
                &request.tracking_number,
            )
            .await
            .map_err(repo_error)
    }

    pub(crate) async fn affiliate_settlements(
        &self,
        mut request: pb::AffiliateSettlementRequest,
    ) -> Result<pb::AffiliateSettlementListResponse, OrderError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.cursor = request.cursor.take().map(|value| value.trim().to_string());
        validate_merchant_id(&request.merchant_id)?;
        validate_cursor(request.cursor.as_deref())?;
        let limit = usize::try_from(request.limit.unwrap_or(50).clamp(1, 100)).unwrap_or(100);
        let items = self
            .dao
            .settlements(
                &request.merchant_id,
                request.status,
                request.cursor.as_deref(),
                limit,
            )
            .await
            .map_err(repo_error)?;
        let next_cursor = items
            .last()
            .map(|item| item.id.clone())
            .filter(|_| items.len() == limit);
        Ok(pb::AffiliateSettlementListResponse { items, next_cursor })
    }

    pub(crate) async fn settle_affiliate(
        &self,
        mut request: pb::SettleAffiliateRequest,
    ) -> Result<pb::AffiliateSettlement, OrderError> {
        request.merchant_id = request.merchant_id.trim().to_string();
        request.settlement_id = request.settlement_id.trim().to_string();
        validate_merchant_id(&request.merchant_id)?;
        if invalid_identifier(&request.settlement_id) {
            return Err(OrderError::Validation(
                "settlement id is required".to_string(),
            ));
        }
        self.dao
            .settle_affiliate(&request.merchant_id, &request.settlement_id)
            .await
            .map_err(repo_error)
    }

    /// Refund-path hook: reverses an order's affiliate share. The money
    /// movement itself belongs to the (future) refund channel; until it ships,
    /// this RPC stands ready as the idempotent ledger entry point and never
    /// appears on merchant-facing routes.
    pub(crate) async fn reverse_affiliate(
        &self,
        mut request: pb::ReverseAffiliateRequest,
    ) -> Result<pb::AffiliateSettlement, OrderError> {
        request.order_id = request.order_id.trim().to_string();
        if invalid_identifier(&request.order_id) {
            return Err(OrderError::Validation("order id is required".to_string()));
        }
        self.dao
            .reverse_affiliate(&request.order_id)
            .await
            .map_err(repo_error)
    }

    pub(crate) async fn cancel(&self, request: pb::OrderRequest) -> Result<pb::Order, OrderError> {
        let mut request = request;
        request.user_id = request.user_id.trim().to_string();
        request.order_id = request.order_id.trim().to_string();
        validate_order_identity(&request.user_id, &request.order_id)?;
        let order = self.get_by_id(&request.user_id, &request.order_id).await?;
        if order.status == pb::MallOrderStatus::Cancelled as i32
            || order.status == pb::MallOrderStatus::Expired as i32
        {
            return Ok(order);
        }
        if order.status == pb::MallOrderStatus::Paid as i32 {
            return Err(OrderError::State(
                "a paid order cannot be cancelled here".to_string(),
            ));
        }
        if order.status == pb::MallOrderStatus::PaymentProcessing as i32 {
            return Err(OrderError::State(
                "payment confirmation is in progress".to_string(),
            ));
        }
        // Claim cancellation while the order is still pending. A payment
        // confirmation can only start from that same state, so whichever
        // transition wins owns the reservation lifecycle and the losing
        // operation must observe the durable state instead of compensating it.
        let cancelled = match self
            .dao
            .transition(
                &request.order_id,
                pb::MallOrderStatus::Cancelled as i32,
                None,
            )
            .await
        {
            Ok(cancelled) => cancelled,
            Err(error) => {
                let current = self.get_by_id(&request.user_id, &request.order_id).await?;
                if matches!(
                    current.status,
                    value if value == pb::MallOrderStatus::PaymentProcessing as i32
                        || value == pb::MallOrderStatus::Paid as i32
                        || value == pb::MallOrderStatus::Cancelled as i32
                        || value == pb::MallOrderStatus::Expired as i32
                ) {
                    return Ok(current);
                }
                return Err(repo_error(error));
            }
        };
        // Release after the state claim. If the inventory call is retried, the
        // order is already durably cancelled and payment can never commit it.
        let reservation = self.release_reservation(&request.order_id).await?;
        if reservation.status == "committed" {
            // Defensive reconciliation for an inventory implementation that
            // reports a late commit; never manufacture a cancelled response.
            return self.get_by_id(&request.user_id, &request.order_id).await;
        }
        Ok(cancelled)
    }

    pub(crate) async fn expire_pending(
        &self,
        request: pb::BatchRequest,
    ) -> Result<pb::ExpirePendingResponse, OrderError> {
        let orders = self
            .dao
            .expired_pending(usize::try_from(request.limit.clamp(1, 1_000)).unwrap_or(1_000))
            .await
            .map_err(repo_error)?;
        let scanned = u32::try_from(orders.len()).unwrap_or(u32::MAX);
        let mut expired = 0_u32;
        let mut failed = 0_u32;
        for order in orders {
            match self.expire_if_needed(order).await {
                Ok(result) if result.status == pb::MallOrderStatus::Expired as i32 => {
                    expired = expired.saturating_add(1);
                }
                Ok(_) => {}
                Err(error) => {
                    failed = failed.saturating_add(1);
                    tracing::warn!(error = %error, "expired mall order reconciliation failed");
                }
            }
        }
        Ok(pb::ExpirePendingResponse {
            scanned,
            expired,
            failed,
        })
    }

    async fn get_by_id(&self, user_id: &str, id: &str) -> Result<pb::Order, OrderError> {
        self.expire_if_needed(self.dao.get(user_id, id).await.map_err(repo_error)?)
            .await
    }

    async fn resume_created(&self, result: CreateResult) -> Result<pb::Order, OrderError> {
        if result.order.status == pb::MallOrderStatus::PendingPayment as i32 {
            self.reserve_inventory(
                &result.order.id,
                reservation_lines(&result.order),
                reservation_ttl_seconds(&result.order.expires_at),
            )
            .await?;
        }
        Ok(result.order)
    }

    async fn resume_pending(&self, order: pb::Order) -> Result<pb::Order, OrderError> {
        let order = self.expire_if_needed(order).await?;
        if order.status == pb::MallOrderStatus::PendingPayment as i32 {
            self.reserve_inventory(
                &order.id,
                reservation_lines(&order),
                reservation_ttl_seconds(&order.expires_at),
            )
            .await?;
        }
        Ok(order)
    }

    async fn expire_if_needed(&self, order: pb::Order) -> Result<pb::Order, OrderError> {
        if !matches!(
            order.status,
            value if value == pb::MallOrderStatus::PendingPayment as i32
                || value == pb::MallOrderStatus::PaymentProcessing as i32
        ) || !expired(&order.expires_at)
        {
            return Ok(order);
        }
        let reservation = self.release_reservation(&order.id).await?;
        if reservation.status == "committed" {
            // Payment won the inventory race. Leave the order in processing so
            // its final payment transition can complete without being
            // overwritten by expiry.
            return self
                .dao
                .get(&order.user_id, &order.id)
                .await
                .map_err(repo_error);
        }
        match self
            .dao
            .transition(&order.id, pb::MallOrderStatus::Expired as i32, None)
            .await
        {
            Ok(value) => Ok(value),
            Err(error) => {
                let current = self
                    .dao
                    .get(&order.user_id, &order.id)
                    .await
                    .map_err(repo_error)?;
                if !matches!(
                    current.status,
                    value if value == pb::MallOrderStatus::PendingPayment as i32
                        || value == pb::MallOrderStatus::PaymentProcessing as i32
                ) {
                    return Ok(current);
                }
                Err(repo_error(error))
            }
        }
    }

    async fn catalog_skus(&self, ids: Vec<String>) -> Result<Vec<mall_pb::MallSku>, OrderError> {
        let mut client = self.mall.clone();
        Ok(client
            .skus(service_request("mall", mall_pb::SkuIdsRequest { ids })?)
            .await
            .map_err(|error| upstream_status("mall", error))?
            .into_inner()
            .items)
    }

    async fn checkout_node_offer(&self, id: &str) -> Result<mall_pb::NodeOffer, OrderError> {
        let mut client = self.mall.clone();
        client
            .get_checkout_node_offer(service_request(
                "mall",
                mall_pb::IdRequest { id: id.to_string() },
            )?)
            .await
            .map_err(|error| upstream_status("mall", error))
            .map(|response| response.into_inner())
    }

    async fn reserve_inventory(
        &self,
        reservation_id: &str,
        items: Vec<inventory_pb::ReservationLine>,
        ttl_seconds: u64,
    ) -> Result<(), OrderError> {
        let mut client = self.inventory.clone();
        client
            .reserve(service_request(
                "mall-inventory",
                inventory_pb::ReserveRequest {
                    reservation_id: reservation_id.to_string(),
                    items,
                    ttl_seconds: Some(ttl_seconds),
                },
            )?)
            .await
            .map_err(|error| upstream_status("mall-inventory", error))?;
        Ok(())
    }

    async fn confirm_reservation(
        &self,
        reservation_id: &str,
    ) -> Result<inventory_pb::Reservation, OrderError> {
        let mut client = self.inventory.clone();
        client
            .confirm(service_request(
                "mall-inventory",
                inventory_pb::IdRequest {
                    id: reservation_id.to_string(),
                },
            )?)
            .await
            .map_err(|error| upstream_status("mall-inventory", error))
            .map(|response| response.into_inner())
    }

    async fn release_reservation(
        &self,
        reservation_id: &str,
    ) -> Result<inventory_pb::Reservation, OrderError> {
        let mut client = self.inventory.clone();
        let reservation = client
            .release(service_request(
                "mall-inventory",
                inventory_pb::IdRequest {
                    id: reservation_id.to_string(),
                },
            )?)
            .await
            .map_err(|error| upstream_status("mall-inventory", error))?
            .into_inner();
        Ok(reservation)
    }
}

fn validate_items(items: &[pb::OrderItemRequest]) -> Result<(), OrderError> {
    if items.is_empty()
        || items.len() > 100
        || items
            .iter()
            .any(|item| invalid_identifier(&item.sku_id) || item.quantity == 0)
    {
        return Err(OrderError::Validation(
            "1-100 positive SKU quantities are required".to_string(),
        ));
    }
    if items
        .iter()
        .map(|item| &item.sku_id)
        .collect::<BTreeSet<_>>()
        .len()
        != items.len()
    {
        return Err(OrderError::Validation(
            "duplicate SKU lines must be combined".to_string(),
        ));
    }
    Ok(())
}

fn validate_merchant_id(value: &str) -> Result<(), OrderError> {
    if invalid_identifier(value) {
        Err(OrderError::Validation(
            "merchant id is required".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn validate_order_identity(user_id: &str, order_id: &str) -> Result<(), OrderError> {
    if invalid_identifier(user_id) || invalid_identifier(order_id) {
        return Err(OrderError::Validation(
            "user id and order id are required".to_string(),
        ));
    }
    Ok(())
}

fn validate_cursor(cursor: Option<&str>) -> Result<(), OrderError> {
    if cursor.is_some_and(|value| value.chars().count() > MAX_IDENTIFIER_LENGTH) {
        return Err(OrderError::Validation("cursor is too long".to_string()));
    }
    Ok(())
}

fn invalid_identifier(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.chars().count() > MAX_IDENTIFIER_LENGTH
}

fn contextual_order_item<'a>(
    items: &'a [pb::OrderItemRequest],
    offer: &mall_pb::NodeOffer,
) -> Result<&'a pb::OrderItemRequest, OrderError> {
    // A checkout has one NodeOffer attribution. Accepting extra arbitrary SKUs
    // would recreate a generic catalog cart with only a decorative offer ID.
    let [item] = items else {
        return Err(OrderError::Validation(
            "contextual checkout accepts exactly one offered SKU".to_string(),
        ));
    };
    if item.sku_id != offer.sku_id {
        return Err(OrderError::Validation(
            "node offer SKU must match the checkout SKU".to_string(),
        ));
    }
    Ok(item)
}

fn request_fingerprint(request: &pb::CreateRequest) -> String {
    let mut lines = request
        .items
        .iter()
        .map(|item| (item.sku_id.as_str(), item.quantity))
        .collect::<Vec<_>>();
    lines.sort_unstable();
    let items = lines
        .into_iter()
        .map(|(sku_id, quantity)| format!("{sku_id}:{quantity}"))
        .collect::<Vec<_>>()
        .join("|");
    format!("{items}|offer:{}", request.node_offer_id)
}

fn new_order(
    user_id: &str,
    requested_items: &[pb::OrderItemRequest],
    skus: BTreeMap<String, mall_pb::MallSku>,
    ttl_seconds: u64,
    node_offer: &mall_pb::NodeOffer,
) -> Result<pb::Order, OrderError> {
    let mut total = 0_i64;
    let mut currency = None;
    let mut items = Vec::with_capacity(requested_items.len());
    for requested in requested_items {
        let sku = skus
            .get(&requested.sku_id)
            .ok_or_else(|| OrderError::State(format!("SKU {} is unavailable", requested.sku_id)))?;
        if let Some(current) = &currency {
            if current != &sku.currency {
                return Err(OrderError::State(
                    "all order lines must use one currency".to_string(),
                ));
            }
        } else {
            currency = Some(sku.currency.clone());
        }
        let line_total = sku
            .price_cents
            .checked_mul(i64::from(requested.quantity))
            .ok_or_else(|| OrderError::Validation("order total overflow".to_string()))?;
        total = total
            .checked_add(line_total)
            .ok_or_else(|| OrderError::Validation("order total overflow".to_string()))?;
        items.push(pb::OrderLine {
            sku_id: sku.id.clone(),
            product_id: sku.product_id.clone(),
            title: sku.title.clone(),
            quantity: requested.quantity,
            unit_price_cents: sku.price_cents,
            currency: sku.currency.clone(),
            line_total_cents: line_total,
        });
    }
    let now = OffsetDateTime::now_utc();
    let commission_cents = items
        .iter()
        .find(|item| item.sku_id == node_offer.sku_id)
        .and_then(|item| {
            item.line_total_cents
                .checked_mul(i64::from(node_offer.commission_bps))
        })
        .and_then(|value| value.checked_div(10_000))
        .unwrap_or(0);
    Ok(pb::Order {
        id: Uuid::now_v7().to_string(),
        user_id: user_id.to_string(),
        status: pb::MallOrderStatus::PendingPayment as i32,
        currency: currency.unwrap_or_else(|| "CNY".to_string()),
        total_cents: total,
        items,
        payment_reference: None,
        expires_at: timestamp(
            now + Duration::seconds(i64::try_from(ttl_seconds.clamp(60, 3_600)).unwrap_or(900)),
        ),
        created_at: timestamp(now),
        updated_at: timestamp(now),
        node_offer_id: node_offer.id.clone(),
        affiliate_creator_id: node_offer.creator_id.clone(),
        commission_cents,
        merchant_id: node_offer.merchant_id.clone(),
        fulfillment_status: pb::FulfillmentStatus::Pending as i32,
        tracking_number: String::new(),
    })
}

fn reservation_lines(order: &pb::Order) -> Vec<inventory_pb::ReservationLine> {
    order
        .items
        .iter()
        .map(|item| inventory_pb::ReservationLine {
            sku_id: item.sku_id.clone(),
            quantity: item.quantity,
        })
        .collect()
}

fn reservation_ttl_seconds(expires_at: &str) -> u64 {
    let seconds = OffsetDateTime::parse(expires_at, &Rfc3339)
        .map(|value| (value - OffsetDateTime::now_utc()).whole_seconds())
        .unwrap_or(60);
    u64::try_from(seconds.saturating_add(1))
        .unwrap_or(60)
        .clamp(60, 3_600)
}

fn expired(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|value| value <= OffsetDateTime::now_utc())
        .unwrap_or(false)
}

fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}

fn service_request<T>(service: &'static str, value: T) -> Result<tonic::Request<T>, OrderError> {
    bookway_runtime::grpc_service_request(value)
        .map_err(|error| OrderError::Upstream(service, error.to_string()))
}

fn upstream_status(service: &'static str, error: tonic::Status) -> OrderError {
    OrderError::Upstream(service, error.to_string())
}

fn repo_error(error: DaoError) -> OrderError {
    match error {
        DaoError::NotFound(value) => OrderError::NotFound(value),
        DaoError::Conflict(value) => OrderError::Conflict(value),
        DaoError::State(value) => OrderError::State(value),
        DaoError::Failed(value) => OrderError::Repository(value),
    }
}

#[cfg(test)]
mod tests {
    use super::{OrderError, contextual_order_item};
    use crate::api::pb;
    use bookway_mall_api::pb as mall_pb;

    #[test]
    fn contextual_checkout_rejects_unattributed_cart_lines() {
        let offer = mall_pb::NodeOffer {
            sku_id: "sku-offer".to_string(),
            ..Default::default()
        };
        let offered = pb::OrderItemRequest {
            sku_id: "sku-offer".to_string(),
            quantity: 1,
        };
        assert!(contextual_order_item(std::slice::from_ref(&offered), &offer).is_ok());
        assert!(matches!(
            contextual_order_item(
                &[
                    offered,
                    pb::OrderItemRequest {
                        sku_id: "sku-unattributed".to_string(),
                        quantity: 1,
                    },
                ],
                &offer,
            ),
            Err(OrderError::Validation(_))
        ));
    }
}
