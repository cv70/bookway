use crate::api::pb;
use async_trait::async_trait;
use std::collections::HashMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

#[derive(Debug)]
pub(crate) enum DaoError {
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
pub(crate) trait OrderDao: Send + Sync {
    async fn find_idempotent(
        &self,
        user_id: &str,
        key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Order>, DaoError>;
    async fn create(&self, draft: NewOrder) -> Result<CreateResult, DaoError>;
    async fn get(&self, user_id: &str, id: &str) -> Result<pb::Order, DaoError>;
    async fn list(&self, user_id: &str) -> Result<Vec<pb::Order>, DaoError>;
    async fn expired_pending(&self, limit: usize) -> Result<Vec<pb::Order>, DaoError>;
    async fn claim_payment_reference(
        &self,
        id: &str,
        payment_reference: &str,
    ) -> Result<pb::Order, DaoError>;
    async fn transition(
        &self,
        id: &str,
        status: i32,
        payment_reference: Option<String>,
    ) -> Result<pb::Order, DaoError>;
    async fn merchant_orders(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::Order>, DaoError>;
    async fn update_fulfillment(
        &self,
        merchant_id: &str,
        order_id: &str,
        status: i32,
        tracking_number: &str,
    ) -> Result<pb::Order, DaoError>;
    async fn ensure_settlement(&self, order: &pb::Order) -> Result<(), DaoError>;
    async fn settlements(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::AffiliateSettlement>, DaoError>;
    async fn settle_affiliate(
        &self,
        merchant_id: &str,
        settlement_id: &str,
    ) -> Result<pb::AffiliateSettlement, DaoError>;
}
#[derive(Clone)]
struct MemoryOrder {
    order: pb::Order,
    request_fingerprint: String,
}
fn status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::MallOrderStatus::try_from(value).ok() {
        Some(pb::MallOrderStatus::PendingPayment) => Ok("pending_payment"),
        Some(pb::MallOrderStatus::Paid) => Ok("paid"),
        Some(pb::MallOrderStatus::Cancelled) => Ok("cancelled"),
        Some(pb::MallOrderStatus::Expired) => Ok("expired"),
        None => Err(DaoError::Failed("invalid order status".to_string())),
    }
}
fn parse_status(value: &str) -> Result<i32, DaoError> {
    match value {
        "pending_payment" => Ok(pb::MallOrderStatus::PendingPayment as i32),
        "paid" => Ok(pb::MallOrderStatus::Paid as i32),
        "cancelled" => Ok(pb::MallOrderStatus::Cancelled as i32),
        "expired" => Ok(pb::MallOrderStatus::Expired as i32),
        _ => Err(DaoError::Failed(format!("unknown order status {value}"))),
    }
}
fn fulfillment_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::FulfillmentStatus::try_from(value).ok() {
        Some(pb::FulfillmentStatus::Pending) => Ok("pending"),
        Some(pb::FulfillmentStatus::Processing) => Ok("processing"),
        Some(pb::FulfillmentStatus::Shipped) => Ok("shipped"),
        Some(pb::FulfillmentStatus::Delivered) => Ok("delivered"),
        Some(pb::FulfillmentStatus::Cancelled) => Ok("cancelled"),
        None => Err(DaoError::Failed("invalid fulfillment status".to_string())),
    }
}
fn parse_fulfillment_status(value: &str) -> Result<i32, DaoError> {
    match value {
        "pending" => Ok(pb::FulfillmentStatus::Pending as i32),
        "processing" => Ok(pb::FulfillmentStatus::Processing as i32),
        "shipped" => Ok(pb::FulfillmentStatus::Shipped as i32),
        "delivered" => Ok(pb::FulfillmentStatus::Delivered as i32),
        "cancelled" => Ok(pb::FulfillmentStatus::Cancelled as i32),
        _ => Err(DaoError::Failed(format!(
            "unknown fulfillment status {value}"
        ))),
    }
}
fn settlement_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::AffiliateSettlementStatus::try_from(value).ok() {
        Some(pb::AffiliateSettlementStatus::Pending) => Ok("pending"),
        Some(pb::AffiliateSettlementStatus::Eligible) => Ok("eligible"),
        Some(pb::AffiliateSettlementStatus::Settled) => Ok("settled"),
        Some(pb::AffiliateSettlementStatus::Reversed) => Ok("reversed"),
        None => Err(DaoError::Failed("invalid settlement status".to_string())),
    }
}
fn parse_settlement_status(value: &str) -> Result<i32, DaoError> {
    match value {
        "pending" => Ok(pb::AffiliateSettlementStatus::Pending as i32),
        "eligible" => Ok(pb::AffiliateSettlementStatus::Eligible as i32),
        "settled" => Ok(pb::AffiliateSettlementStatus::Settled as i32),
        "reversed" => Ok(pb::AffiliateSettlementStatus::Reversed as i32),
        _ => Err(DaoError::Failed(format!(
            "unknown settlement status {value}"
        ))),
    }
}
fn validate_fulfillment_transition(current: i32, next: i32) -> Result<(), DaoError> {
    let valid = matches!(
        (
            pb::FulfillmentStatus::try_from(current).ok(),
            pb::FulfillmentStatus::try_from(next).ok()
        ),
        (
            Some(pb::FulfillmentStatus::Pending),
            Some(pb::FulfillmentStatus::Processing)
        ) | (
            Some(pb::FulfillmentStatus::Pending),
            Some(pb::FulfillmentStatus::Cancelled)
        ) | (
            Some(pb::FulfillmentStatus::Processing),
            Some(pb::FulfillmentStatus::Shipped)
        ) | (
            Some(pb::FulfillmentStatus::Processing),
            Some(pb::FulfillmentStatus::Cancelled)
        ) | (
            Some(pb::FulfillmentStatus::Shipped),
            Some(pb::FulfillmentStatus::Delivered)
        ) | (
            Some(pb::FulfillmentStatus::Shipped),
            Some(pb::FulfillmentStatus::Cancelled)
        ),
    ) || current == next;
    if valid {
        Ok(())
    } else {
        Err(DaoError::State(
            "invalid fulfillment state transition".to_string(),
        ))
    }
}
fn parse_timestamp(value: &str) -> Result<OffsetDateTime, DaoError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| DaoError::Failed(error.to_string()))
}
fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
fn database(error: sqlx::Error) -> DaoError {
    DaoError::Failed(error.to_string())
}
fn payment_reference_error(error: sqlx::Error) -> DaoError {
    if let sqlx::Error::Database(value) = &error
        && value.code().as_deref() == Some("23505")
    {
        return DaoError::Conflict("payment reference belongs to a different order".to_string());
    }
    database(error)
}

#[cfg(test)]
mod tests {
    use super::{DaoError, MemoryOrderDao, NewOrder, OrderDao};
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
            node_offer_id: "offer-1".to_string(),
            affiliate_creator_id: "creator-1".to_string(),
            commission_cents: 0,
            merchant_id: "merchant-1".to_string(),
            fulfillment_status: pb::FulfillmentStatus::Pending as i32,
            tracking_number: String::new(),
        }
    }

    #[tokio::test]
    async fn idempotency_key_rejects_a_different_order_payload() {
        let dao = MemoryOrderDao::default();
        dao.create(NewOrder {
            order: draft("order-1"),
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("initial order should be created");
        let retry = dao
            .find_idempotent("user-1", "key-1", "sku-1:1")
            .await
            .expect("matching payload should replay the order");
        assert_eq!(retry.expect("order should exist").id, "order-1");
        let error = dao
            .find_idempotent("user-1", "key-1", "sku-2:1")
            .await
            .expect_err("different payload must not reuse the order");
        assert!(matches!(error, DaoError::Conflict(_)));
    }

    #[tokio::test]
    async fn payment_reference_cannot_be_claimed_by_two_orders() {
        let dao = MemoryOrderDao::default();
        dao.create(NewOrder {
            order: draft("order-1"),
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("first order should be created");
        dao.create(NewOrder {
            order: draft("order-2"),
            idempotency_key: "key-2".to_string(),
            request_fingerprint: "sku-2:1".to_string(),
        })
        .await
        .expect("second order should be created");
        dao.claim_payment_reference("order-1", "payment-1")
            .await
            .expect("first claim should succeed");
        let error = dao
            .claim_payment_reference("order-2", "payment-1")
            .await
            .expect_err("payment reference reuse must fail");
        assert!(matches!(error, DaoError::Conflict(_)));
    }

    #[tokio::test]
    async fn payment_reference_cannot_be_claimed_after_the_order_expires() {
        let dao = MemoryOrderDao::default();
        let mut order = draft("order-1");
        order.expires_at = "2000-01-01T00:00:00Z".to_string();
        dao.create(NewOrder {
            order,
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("order should be created");
        let error = dao
            .claim_payment_reference("order-1", "payment-1")
            .await
            .expect_err("expired orders cannot start payment");
        assert!(matches!(error, DaoError::State(_)));
    }
}

#[path = "memory_order_dao.rs"]
mod memory_order_dao;
pub(crate) use memory_order_dao::MemoryOrderDao;
#[path = "postgres_order_dao.rs"]
mod postgres_order_dao;
pub(crate) use postgres_order_dao::PostgresOrderDao;
