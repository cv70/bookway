use crate::api::pb;
use crate::{
    Config,
    datasource::{
        CreateResult, MemoryOrderRepository, NewOrder, OrderRepository, PostgresOrderRepository,
        RepositoryError,
    },
};
use bookway_mall_api::pb as mall_pb;
use bookway_mall_inventory_api::pb as inventory_pb;
use bookway_user_event_api::pb as user_event_pb;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tonic::transport::Channel;
use uuid::Uuid;

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
    repository: Arc<dyn OrderRepository>,
    mall: mall_pb::mall_client::MallClient<Channel>,
    inventory: inventory_pb::mall_inventory_client::MallInventoryClient<Channel>,
    user_event: user_event_pb::user_event_client::UserEventClient<Channel>,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let repository: Arc<dyn OrderRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryOrderRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresOrderRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let mall = mall_pb::mall_client::MallClient::connect(config.mall_url.clone()).await?;
        let inventory = inventory_pb::mall_inventory_client::MallInventoryClient::connect(
            config.inventory_url.clone(),
        )
        .await?;
        let user_event = user_event_pb::user_event_client::UserEventClient::connect(
            config.user_event_url.clone(),
        )
        .await?;
        Ok(Self {
            config,
            repository,
            mall,
            inventory,
            user_event,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) async fn create(&self, request: pb::CreateRequest) -> Result<pb::Order, OrderError> {
        if request.user_id.trim().is_empty() || request.idempotency_key.trim().is_empty() {
            return Err(OrderError::Validation(
                "user_id and Idempotency-Key are required".to_string(),
            ));
        }
        validate_items(&request.items)?;
        let request_fingerprint = request_fingerprint(&request);
        if let Some(order) = self
            .repository
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
        let node_offer = match request.node_offer_id.as_deref() {
            Some(id) if !id.trim().is_empty() => Some(self.node_offer(id).await?),
            Some(_) => {
                return Err(OrderError::Validation(
                    "node offer id cannot be empty".to_string(),
                ));
            }
            None => None,
        };
        if let Some(offer) = &node_offer {
            let Some(item) = request
                .items
                .iter()
                .find(|item| item.sku_id == offer.sku_id)
            else {
                return Err(OrderError::Validation(
                    "node offer SKU must be included in the order".to_string(),
                ));
            };
            let Some(sku) = sku_map.get(&item.sku_id) else {
                return Err(OrderError::State(
                    "node offer SKU is unavailable".to_string(),
                ));
            };
            if offer.product_id != sku.product_id {
                return Err(OrderError::Conflict(
                    "node offer product does not match the SKU".to_string(),
                ));
            }
            if offer.commission_bps > 3_000 || offer.creator_id.trim().is_empty() {
                return Err(OrderError::State(
                    "node offer commission metadata is invalid".to_string(),
                ));
            }
        }
        let order = new_order(
            &request.user_id,
            &request.items,
            sku_map,
            self.config.payment_ttl_seconds,
            node_offer.as_ref(),
        )?;
        let created = self
            .repository
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
        request: pb::UserRequest,
    ) -> Result<pb::OrderListResponse, OrderError> {
        let values = self
            .repository
            .list(&request.user_id)
            .await
            .map_err(repo_error)?;
        let mut items = Vec::with_capacity(values.len());
        for value in values {
            items.push(self.expire_if_needed(value).await?);
        }
        Ok(pb::OrderListResponse { items })
    }

    pub(crate) async fn get(&self, request: pb::OrderRequest) -> Result<pb::Order, OrderError> {
        self.get_by_id(&request.user_id, &request.order_id).await
    }

    pub(crate) async fn pay(&self, request: pb::PayRequest) -> Result<pb::Order, OrderError> {
        if request.payment_reference.trim().is_empty() {
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
            self.record_contextual_purchase(&order).await;
            return Ok(order);
        }
        if order.status != pb::MallOrderStatus::PendingPayment as i32 {
            return Err(OrderError::State(format!(
                "order {} cannot be paid from its current state",
                request.order_id
            )));
        }
        self.repository
            .claim_payment_reference(&request.order_id, &request.payment_reference)
            .await
            .map_err(repo_error)?;
        self.confirm_reservation(&request.order_id).await?;
        let paid = self
            .repository
            .transition(&request.order_id, pb::MallOrderStatus::Paid as i32, None)
            .await
            .map_err(repo_error)?;
        self.record_contextual_purchase(&paid).await;
        Ok(paid)
    }

    pub(crate) async fn cancel(&self, request: pb::OrderRequest) -> Result<pb::Order, OrderError> {
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
        self.release_reservation(&request.order_id).await?;
        self.repository
            .transition(
                &request.order_id,
                pb::MallOrderStatus::Cancelled as i32,
                None,
            )
            .await
            .map_err(repo_error)
    }

    pub(crate) async fn expire_pending(
        &self,
        request: pb::BatchRequest,
    ) -> Result<pb::ExpirePendingResponse, OrderError> {
        let orders = self
            .repository
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
        self.expire_if_needed(self.repository.get(user_id, id).await.map_err(repo_error)?)
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
        if order.status != pb::MallOrderStatus::PendingPayment as i32 || !expired(&order.expires_at)
        {
            return Ok(order);
        }
        self.release_reservation(&order.id).await?;
        self.repository
            .transition(&order.id, pb::MallOrderStatus::Expired as i32, None)
            .await
            .map_err(repo_error)
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

    async fn node_offer(&self, id: &str) -> Result<mall_pb::NodeOffer, OrderError> {
        let mut client = self.mall.clone();
        client
            .get_node_offer(service_request(
                "mall",
                mall_pb::IdRequest { id: id.to_string() },
            )?)
            .await
            .map_err(|error| upstream_status("mall", error))
            .map(|response| response.into_inner())
    }

    async fn record_contextual_purchase(&self, order: &pb::Order) {
        let Some(offer_id) = order.node_offer_id.as_deref() else {
            return;
        };
        let offer = match self.node_offer(offer_id).await {
            Ok(offer) => offer,
            Err(error) => {
                tracing::warn!(%error, order_id = %order.id, "contextual offer lookup degraded after payment");
                return;
            }
        };
        let Some(event) = contextual_purchase_event(&order.user_id, &order.id, &offer.route_id)
        else {
            tracing::warn!(order_id = %order.id, "contextual purchase timestamp formatting failed");
            return;
        };
        let mut client = self.user_event.clone();
        let request = match service_request(
            "user-event",
            user_event_pb::IngestRequest {
                user_id: order.user_id.clone(),
                events: vec![event],
            },
        ) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, order_id = %order.id, "contextual purchase request setup failed");
                return;
            }
        };
        if let Err(error) = client.ingest(request).await {
            tracing::warn!(%error, order_id = %order.id, "contextual purchase attribution degraded");
        }
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

    async fn confirm_reservation(&self, reservation_id: &str) -> Result<(), OrderError> {
        let mut client = self.inventory.clone();
        client
            .confirm(service_request(
                "mall-inventory",
                inventory_pb::IdRequest {
                    id: reservation_id.to_string(),
                },
            )?)
            .await
            .map_err(|error| upstream_status("mall-inventory", error))?;
        Ok(())
    }

    async fn release_reservation(&self, reservation_id: &str) -> Result<(), OrderError> {
        let mut client = self.inventory.clone();
        client
            .release(service_request(
                "mall-inventory",
                inventory_pb::IdRequest {
                    id: reservation_id.to_string(),
                },
            )?)
            .await
            .map_err(|error| upstream_status("mall-inventory", error))?;
        Ok(())
    }
}

fn validate_items(items: &[pb::OrderItemRequest]) -> Result<(), OrderError> {
    if items.is_empty()
        || items.len() > 100
        || items
            .iter()
            .any(|item| item.sku_id.trim().is_empty() || item.quantity == 0)
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
    format!(
        "{items}|offer:{}",
        request.node_offer_id.as_deref().unwrap_or_default()
    )
}

fn new_order(
    user_id: &str,
    requested_items: &[pb::OrderItemRequest],
    skus: BTreeMap<String, mall_pb::MallSku>,
    ttl_seconds: u64,
    node_offer: Option<&mall_pb::NodeOffer>,
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
    let commission_cents = node_offer
        .and_then(|offer| {
            items
                .iter()
                .find(|item| item.sku_id == offer.sku_id)
                .and_then(|item| {
                    item.line_total_cents
                        .checked_mul(i64::from(offer.commission_bps))
                })
                .and_then(|value| value.checked_div(10_000))
        })
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
        node_offer_id: node_offer.map(|offer| offer.id.clone()),
        affiliate_creator_id: node_offer.map(|offer| offer.creator_id.clone()),
        commission_cents,
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

fn contextual_purchase_event(
    user_id: &str,
    order_id: &str,
    route_id: &str,
) -> Option<user_event_pb::Event> {
    let occurred_at = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    let stable_key = format!("bookway:contextual-purchase:{user_id}:{order_id}");
    Some(user_event_pb::Event {
        event_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
        event_type: "purchase".to_string(),
        session_id: "server".to_string(),
        request_id: None,
        component_id: "contextual-commerce".to_string(),
        content_id: Some(route_id.to_string()),
        position: None,
        occurred_at,
        source: "mall-order".to_string(),
        attribution_source: user_event_pb::AttributionSource::Unspecified as i32,
        negative_feedback_reason: None,
    })
}

fn service_request<T>(service: &'static str, value: T) -> Result<tonic::Request<T>, OrderError> {
    bookway_runtime::grpc_service_request(value)
        .map_err(|error| OrderError::Upstream(service, error.to_string()))
}

fn upstream_status(service: &'static str, error: tonic::Status) -> OrderError {
    OrderError::Upstream(service, error.to_string())
}

fn repo_error(error: RepositoryError) -> OrderError {
    match error {
        RepositoryError::NotFound(value) => OrderError::NotFound(value),
        RepositoryError::Conflict(value) => OrderError::Conflict(value),
        RepositoryError::State(value) => OrderError::State(value),
        RepositoryError::Failed(value) => OrderError::Repository(value),
    }
}
