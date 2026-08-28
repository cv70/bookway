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
    async fn begin_payment(&self, id: &str, payment_reference: &str)
    -> Result<pb::Order, DaoError>;
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
    async fn get_by_payment_reference(
        &self,
        reference: &str,
    ) -> Result<Option<(String, String)>, DaoError>;
    async fn ensure_settlement(
        &self,
        order: &pb::Order,
        hold_days: u32,
    ) -> Result<(), DaoError>;
    /// Flips pending creator shares whose refund window has elapsed to
    /// eligible. Returns how many rows were promoted.
    async fn promote_eligible_settlements(&self) -> Result<u64, DaoError>;
    async fn settlements(
        &self,
        merchant_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::AffiliateSettlement>, DaoError>;
    /// Creator-facing read of the same ledger: identical rows, filtered by
    /// `creator_id` instead of `merchant_id`. No write capability ships with
    /// it.
    async fn creator_settlements(
        &self,
        creator_id: &str,
        status: Option<i32>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<pb::AffiliateSettlement>, DaoError>;
    async fn settle_affiliate(
        &self,
        merchant_id: &str,
        settlement_id: &str,
    ) -> Result<pb::AffiliateSettlement, DaoError>;
    /// Refund-path ledger fact: flips the order's settlement from eligible to
    /// reversed. Replaying an already reversed order returns that row; a
    /// settled or pending settlement is not reversible here.
    async fn reverse_affiliate(&self, order_id: &str) -> Result<pb::AffiliateSettlement, DaoError>;
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
        Some(pb::MallOrderStatus::PaymentProcessing) => Ok("payment_processing"),
        Some(pb::MallOrderStatus::PaidAfterExpiry) => Ok("paid_after_expiry"),
        None => Err(DaoError::Failed("invalid order status".to_string())),
    }
}
fn parse_status(value: &str) -> Result<i32, DaoError> {
    match value {
        "pending_payment" => Ok(pb::MallOrderStatus::PendingPayment as i32),
        "paid" => Ok(pb::MallOrderStatus::Paid as i32),
        "cancelled" => Ok(pb::MallOrderStatus::Cancelled as i32),
        "expired" => Ok(pb::MallOrderStatus::Expired as i32),
        "payment_processing" => Ok(pb::MallOrderStatus::PaymentProcessing as i32),
        "paid_after_expiry" => Ok(pb::MallOrderStatus::PaidAfterExpiry as i32),
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
            ad_attribution: None,
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
        dao.begin_payment("order-1", "payment-1")
            .await
            .expect("first claim should succeed");
        assert_eq!(
            dao.get("user-1", "order-1")
                .await
                .expect("order should be readable")
                .status,
            pb::MallOrderStatus::PaymentProcessing as i32
        );
        let error = dao
            .begin_payment("order-2", "payment-1")
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
            .begin_payment("order-1", "payment-1")
            .await
            .expect_err("expired orders cannot start payment");
        assert!(matches!(error, DaoError::State(_)));
    }

    #[tokio::test]
    async fn payment_processing_is_retryable_and_cannot_be_cancelled() {
        let dao = MemoryOrderDao::default();
        dao.create(NewOrder {
            order: draft("order-1"),
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("order should be created");

        dao.begin_payment("order-1", "payment-1")
            .await
            .expect("payment should enter processing");
        let retry = dao
            .begin_payment("order-1", "payment-1")
            .await
            .expect("same payment retry should be idempotent");
        assert_eq!(retry.status, pb::MallOrderStatus::PaymentProcessing as i32);
        let cancel = dao
            .transition("order-1", pb::MallOrderStatus::Cancelled as i32, None)
            .await;
        assert!(matches!(cancel, Err(DaoError::Failed(_))));
    }

    #[tokio::test]
    async fn payment_processing_can_expire() {
        let dao = MemoryOrderDao::default();
        dao.create(NewOrder {
            order: draft("order-1"),
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("order should be created");
        dao.begin_payment("order-1", "payment-1")
            .await
            .expect("payment should enter processing");
        let expired = dao
            .transition("order-1", pb::MallOrderStatus::Expired as i32, None)
            .await
            .expect("processing order should be expirable");
        assert_eq!(expired.status, pb::MallOrderStatus::Expired as i32);
    }

    #[tokio::test]
    async fn committed_payment_processing_can_finish_after_its_order_ttl() {
        let dao = MemoryOrderDao::default();
        let mut order = draft("order-1");
        order.status = pb::MallOrderStatus::PaymentProcessing as i32;
        order.payment_reference = Some("payment-1".to_string());
        order.expires_at = "2000-01-01T00:00:00Z".to_string();
        dao.create(NewOrder {
            order,
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("crash-recovery order should be persisted");

        // Inventory has already committed before the process crashed. The
        // order state machine must permit the durable completion transition,
        // even though the original payment window has elapsed.
        let paid = dao
            .transition("order-1", pb::MallOrderStatus::Paid as i32, None)
            .await
            .expect("committed payment recovery should finish the order");

        assert_eq!(paid.status, pb::MallOrderStatus::Paid as i32);
        assert_eq!(paid.payment_reference.as_deref(), Some("payment-1"));
    }

    #[tokio::test]
    async fn paying_a_contextual_order_enqueues_exactly_one_purchase_event() {
        let dao = MemoryOrderDao::default();
        // draft() carries node_offer_id "offer-1": a contextual order.
        dao.create(NewOrder {
            order: draft("order-ctx"),
            idempotency_key: "key-ctx".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("contextual order should be created");
        begin_and_pay(&dao, "order-ctx", "payment-ctx").await;
        // A paid replay stays paid and must not re-enqueue.
        dao.transition(
            "order-ctx",
            pb::MallOrderStatus::Paid as i32,
            Some("payment-ctx".to_string()),
        )
        .await
        .expect("paid replay is idempotent");

        let mut unattributed = draft("order-plain");
        unattributed.node_offer_id = String::new();
        dao.create(NewOrder {
            order: unattributed,
            idempotency_key: "key-plain".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("unattributed order should be created");
        begin_and_pay(&dao, "order-plain", "payment-plain").await;

        let queue = dao.purchase_queue().await;
        assert_eq!(queue.len(), 1, "one contextual order, one queued event");
        let entry = queue.first().expect("queue must hold the queued event");
        assert_eq!(entry.order_id, "order-ctx");
        assert_eq!(entry.user_id, "user-1");
        assert_eq!(entry.node_offer_id, "offer-1");

        // The pay endpoint re-runs ensure_settlement on every paid replay;
        // the settlement ledger must stay at exactly one eligible row.
        let paid_order = dao
            .get("user-1", "order-ctx")
            .await
            .expect("paid order should be readable");
        dao.ensure_settlement(&paid_order, 0)
            .await
            .expect("paid replay should keep the settlement");
        let settlements = dao
            .settlements("merchant-1", None, None, 10)
            .await
            .expect("list merchant settlements");
        let ctx_settlements = settlements
            .iter()
            .filter(|item| item.order_id == "order-ctx")
            .count();
        assert_eq!(ctx_settlements, 1, "pay replay must not duplicate the settlement");
        assert_eq!(
            settlements
                .iter()
                .find(|item| item.order_id == "order-ctx")
                .map(|item| item.amount_cents),
            Some(0),
            "settlement amount comes from the order commission snapshot"
        );
    }

    #[tokio::test]
    async fn held_settlements_are_not_payable_until_promoted_and_reversible_in_window() {
        let dao = MemoryOrderDao::default();
        let mut order = draft("order-hold");
        order.commission_cents = 500;
        dao.create(NewOrder {
            order,
            idempotency_key: "key-hold".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("order should be created");
        begin_and_pay(&dao, "order-hold", "payment-hold").await;
        let paid_order = dao.get("user-1", "order-hold").await.expect("paid order");
        // The hold keeps the share pending even though the order is paid.
        dao.ensure_settlement(&paid_order, 7)
            .await
            .expect("settlement should exist");

        let pending = dao
            .settlements("merchant-1", Some(pb::AffiliateSettlementStatus::Eligible as i32), None, 10)
            .await
            .expect("list eligible");
        assert!(
            pending.is_empty(),
            "a held share must not be payable during the refund window"
        );

        // An in-window refund voids the pending share before any payout.
        let reversed = dao
            .reverse_affiliate("order-hold")
            .await
            .expect("pending share should reverse");
        assert_eq!(reversed.status, pb::AffiliateSettlementStatus::Reversed as i32);

        // A second order without a hold keeps the legacy immediate eligibility.
        let mut instant = draft("order-instant");
        instant.commission_cents = 300;
        dao.create(NewOrder {
            order: instant,
            idempotency_key: "key-instant".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("order should be created");
        begin_and_pay(&dao, "order-instant", "payment-instant").await;
        let instant_order = dao.get("user-1", "order-instant").await.expect("paid order");
        dao.ensure_settlement(&instant_order, 0)
            .await
            .expect("settlement should exist");
        let eligible = dao
            .settlements("merchant-1", Some(pb::AffiliateSettlementStatus::Eligible as i32), None, 10)
            .await
            .expect("list eligible");
        assert_eq!(
            eligible.iter().filter(|item| item.order_id == "order-instant").count(),
            1,
            "hold-free shares stay immediately eligible"
        );
    }

    #[tokio::test]
    async fn promotion_flips_only_elapsed_pending_shares() {
        let dao = MemoryOrderDao::default();
        let mut order = draft("order-past");
        order.commission_cents = 100;
        dao.create(NewOrder {
            order,
            idempotency_key: "key-1".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("order should be created");
        begin_and_pay(&dao, "order-past", "payment-1").await;
        let paid_order = dao.get("user-1", "order-past").await.expect("paid order");
        // hold_days = 0 writes eligible_at = now, so the very next promotion
        // pass must pick it up.
        dao.ensure_settlement(&paid_order, 0).await.expect("settlement");

        // Force the row into pending with an elapsed window to exercise the
        // promotion predicate directly.
        {
            let mut settlements = dao.settlements_debug().await;
            for item in settlements.values_mut() {
                if item.order_id == "order-past" {
                    item.status = pb::AffiliateSettlementStatus::Pending as i32;
                }
            }
        }
        let promoted = dao
            .promote_eligible_settlements()
            .await
            .expect("promotion should run");
        assert_eq!(promoted, 1);
        let eligible = dao
            .settlements("merchant-1", Some(pb::AffiliateSettlementStatus::Eligible as i32), None, 10)
            .await
            .expect("list eligible");
        assert_eq!(eligible.len(), 1);
    }

    async fn begin_and_pay(dao: &MemoryOrderDao, id: &str, reference: &str) {
        dao.begin_payment(id, reference)
            .await
            .expect("payment should claim the order");
        dao.transition(id, pb::MallOrderStatus::Paid as i32, None)
            .await
            .expect("order should be paid");
    }

    #[tokio::test]
    async fn affiliate_reversal_flips_only_eligible_settlements_and_replays_idempotently() {
        let dao = MemoryOrderDao::default();
        let mut paid = draft("order-1");
        paid.commission_cents = 120;
        dao.ensure_settlement(&paid, 0)
            .await
            .expect("settlement should exist");

        let reversed = dao
            .reverse_affiliate("order-1")
            .await
            .expect("eligible settlement should reverse");
        assert_eq!(
            reversed.status,
            pb::AffiliateSettlementStatus::Reversed as i32
        );
        assert_eq!(reversed.amount_cents, 120);

        let replay = dao
            .reverse_affiliate("order-1")
            .await
            .expect("a reversal replay is idempotent");
        assert_eq!(
            replay.status,
            pb::AffiliateSettlementStatus::Reversed as i32
        );

        // A settled share has already been paid out; the refund money channel,
        // not this ledger hook, owns clawing funds back.
        let mut settled_order = draft("order-2");
        settled_order.commission_cents = 50;
        dao.ensure_settlement(&settled_order, 0)
            .await
            .expect("second settlement should exist");
        let id = dao
            .settlements("merchant-1", None, None, 10)
            .await
            .expect("list merchant settlements")
            .into_iter()
            .find(|item| item.order_id == "order-2")
            .map(|item| item.id)
            .expect("settlement for order-2");
        dao.settle_affiliate("merchant-1", &id)
            .await
            .expect("merchant settles order-2's share");
        let error = dao
            .reverse_affiliate("order-2")
            .await
            .expect_err("a settled share cannot be reversed here");
        assert!(matches!(error, DaoError::State(_)));

        assert!(matches!(
            dao.reverse_affiliate("order-missing").await,
            Err(DaoError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn expired_order_with_a_claimed_reference_reconciles_to_paid_after_expiry() {
        let dao = MemoryOrderDao::default();
        // Crash-recovery style fixture: payment was claimed (the reference is
        // durable) but the TTL elapsed before inventory committed, so the
        // expirer marked the order expired.
        let mut order = draft("order-late");
        order.status = pb::MallOrderStatus::Expired as i32;
        order.payment_reference = Some("payment-late".to_string());
        order.expires_at = "2000-01-01T00:00:00Z".to_string();
        dao.create(NewOrder {
            order,
            idempotency_key: "key-late".to_string(),
            request_fingerprint: "sku-1:1".to_string(),
        })
        .await
        .expect("expired order should be persisted");

        let reconciled = dao
            .transition(
                "order-late",
                pb::MallOrderStatus::PaidAfterExpiry as i32,
                None,
            )
            .await
            .expect("a late provider confirmation must reconcile, not fail forever");
        assert_eq!(
            reconciled.status,
            pb::MallOrderStatus::PaidAfterExpiry as i32
        );
        assert_eq!(
            reconciled.payment_reference.as_deref(),
            Some("payment-late")
        );

        // Webhook retries replay idempotently on the same durable state.
        let replay = dao
            .transition(
                "order-late",
                pb::MallOrderStatus::PaidAfterExpiry as i32,
                None,
            )
            .await
            .expect("replay stays idempotent");
        assert_eq!(replay.status, pb::MallOrderStatus::PaidAfterExpiry as i32);

        // The reconciliation is a one-way ledger fact: no state may overwrite
        // it, and it is not a source for any other transition.
        for forbidden in [
            pb::MallOrderStatus::Paid,
            pb::MallOrderStatus::Cancelled,
            pb::MallOrderStatus::Expired,
            pb::MallOrderStatus::PendingPayment,
        ] {
            let error = dao
                .transition("order-late", forbidden as i32, None)
                .await
                .expect_err("paid_after_expiry is terminal for the service");
            assert!(matches!(error, DaoError::Failed(_)));
        }
        // No affiliate settlement may be created for a paid_after_expiry
        // order: ensure_settlement is only reachable from the paid paths, and
        // the ledger here starts empty for this order.
        let settlements = dao
            .creator_settlements("creator-1", None, None, 10)
            .await
            .expect("creator ledger should read");
        assert!(
            settlements.is_empty(),
            "paid_after_expiry must never mint a settlement row"
        );
    }

    #[tokio::test]
    async fn creator_settlements_list_only_their_own_ledger_rows() {
        let dao = MemoryOrderDao::default();
        for (order_id, creator, amount) in [
            ("order-a", "creator-1", 100_i64),
            ("order-b", "creator-2", 200_i64),
            ("order-c", "creator-1", 300_i64),
        ] {
            let mut order = draft(order_id);
            order.affiliate_creator_id = creator.to_string();
            order.commission_cents = amount;
            dao.ensure_settlement(&order, 0)
                .await
                .expect("settlement should exist");
        }
        // Settle order-a so the status filter has something to exclude.
        let a_id = dao
            .settlements("merchant-1", None, None, 10)
            .await
            .expect("merchant ledger")
            .into_iter()
            .find(|item| item.order_id == "order-a")
            .map(|item| item.id)
            .expect("settlement for order-a");
        dao.settle_affiliate("merchant-1", &a_id)
            .await
            .expect("merchant settles order-a");

        let all = dao
            .creator_settlements("creator-1", None, None, 10)
            .await
            .expect("creator ledger");
        assert_eq!(all.len(), 2, "creator-1 sees exactly their own rows");
        assert!(all.iter().all(|item| item.creator_id == "creator-1"));

        let eligible_only = dao
            .creator_settlements(
                "creator-1",
                Some(pb::AffiliateSettlementStatus::Eligible as i32),
                None,
                10,
            )
            .await
            .expect("filtered ledger");
        assert_eq!(
            eligible_only
                .iter()
                .map(|item| item.order_id.as_str())
                .collect::<Vec<_>>(),
            vec!["order-c"],
            "the settled row is excluded by the status filter"
        );

        // Cursor pagination walks the creator's rows newest-first.
        let first_page = dao
            .creator_settlements("creator-1", None, None, 1)
            .await
            .expect("first page");
        assert_eq!(first_page.len(), 1);
        let next_page = dao
            .creator_settlements("creator-1", None, Some(first_page[0].id.as_str()), 10)
            .await
            .expect("second page");
        assert_eq!(next_page.len(), 1);
        assert_ne!(first_page[0].id, next_page[0].id);

        assert!(
            dao.creator_settlements("creator-unknown", None, None, 10)
                .await
                .expect("unknown creator reads an empty ledger")
                .is_empty()
        );
    }
}

#[path = "memory_order_dao.rs"]
mod memory_order_dao;
pub(crate) use memory_order_dao::MemoryOrderDao;
#[path = "postgres_order_dao.rs"]
mod postgres_order_dao;
pub(crate) use postgres_order_dao::PostgresOrderDao;
