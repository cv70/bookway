use std::{env, time::Duration};

use bookway_mall_api::pb as mall_pb;
use bookway_user_event_api::pb as user_event_pb;
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tonic::transport::{Channel, Endpoint};
use uuid::Uuid;

/// Delivers route-attributed purchase facts to user-event ingest.
///
/// Rows are enqueued by mall-order in the same transaction that marks an
/// order paid (`purchase_event_outbox`), so every paid contextual order is
/// attributed exactly once regardless of process crashes. The relay resolves
/// the offer's route at delivery time and reuses the deterministic event UUID
/// that user-event dedupes on, which makes redelivery safe even after a crash
/// between ingest success and the outbox status update.
pub(crate) struct PurchaseEventRelay {
    pool: PgPool,
    mall: mall_pb::mall_client::MallClient<Channel>,
    user_event: user_event_pb::user_event_client::UserEventClient<Channel>,
    batch_size: i64,
}

enum DeliveryError {
    /// Retrying can never help: the referenced offer carries no attribution.
    Permanent(String),
    /// Transport or upstream failures; retried with exponential backoff.
    Transient(String),
}

struct ClaimedRow {
    order_id: String,
    user_id: String,
    node_offer_id: String,
}

impl PurchaseEventRelay {
    pub(crate) fn from_env(pool: PgPool) -> Result<Self, Box<dyn std::error::Error>> {
        let mall_url = env::var("MALL_GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8101".to_string());
        let user_event_url =
            env::var("USER_EVENT_GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:18089".to_string());
        let batch_size = env::var("OUTBOX_BATCH_SIZE")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100)
            .clamp(1, 1000);
        // Lazy connections keep the relay runnable while services restart;
        // delivery errors surface per row and are retried by the outbox.
        let mall = mall_pb::mall_client::MallClient::new(
            Endpoint::from_shared(mall_url)?.connect_lazy(),
        );
        let user_event = user_event_pb::user_event_client::UserEventClient::new(
            Endpoint::from_shared(user_event_url)?.connect_lazy(),
        );
        Ok(Self {
            pool,
            mall,
            user_event,
            batch_size,
        })
    }

    pub(crate) async fn run_forever(&self) {
        loop {
            match self.run_once().await {
                Ok(0) => tokio::time::sleep(Duration::from_millis(500)).await,
                Ok(count) => tracing::debug!(count, "purchase events ingested"),
                Err(error) => {
                    tracing::error!(%error, "purchase event relay iteration failed");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    async fn run_once(&self) -> Result<usize, sqlx::Error> {
        let rows = self.claim().await?;
        let count = rows.len();
        for row in rows {
            match self.deliver(&row).await {
                Ok(()) => self.mark_delivered(&row.order_id).await?,
                Err(DeliveryError::Permanent(reason)) => {
                    tracing::warn!(order_id = %row.order_id, %reason, "purchase event dead-lettered");
                    self.mark_dead(&row.order_id, &reason).await?;
                }
                Err(DeliveryError::Transient(error)) => {
                    tracing::warn!(order_id = %row.order_id, %error, "purchase event delivery retry scheduled");
                    self.mark_retry(&row.order_id, &error).await?;
                }
            }
        }
        Ok(count)
    }

    async fn claim(&self) -> Result<Vec<ClaimedRow>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "WITH claimed AS (
                SELECT order_id FROM purchase_event_outbox
                WHERE ((status = 'pending' AND available_at <= now())
                    OR (status = 'processing' AND available_at <= now() - interval '5 minutes'))
                ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1
             )
             UPDATE purchase_event_outbox o
             SET status='processing', locked_at=now(), updated_at=now()
             FROM claimed WHERE o.order_id = claimed.order_id
             RETURNING o.order_id, o.user_id, o.node_offer_id",
        )
        .bind(self.batch_size)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|(order_id, user_id, node_offer_id)| ClaimedRow {
                order_id,
                user_id,
                node_offer_id,
            })
            .collect())
    }

    async fn deliver(&self, row: &ClaimedRow) -> Result<(), DeliveryError> {
        let offer_request = bookway_runtime::grpc_service_request(mall_pb::IdRequest {
            id: row.node_offer_id.clone(),
        })
        .map_err(|error| DeliveryError::Transient(error.to_string()))?;
        let offer = self
            .mall
            .clone()
            .get_node_offer(offer_request)
            .await
            .map_err(|status| match status.code() {
                tonic::Code::NotFound => DeliveryError::Permanent(format!(
                    "node offer {} no longer exists",
                    row.node_offer_id
                )),
                _ => DeliveryError::Transient(status.to_string()),
            })?
            .into_inner();
        if offer.route_id.is_empty() {
            return Err(DeliveryError::Permanent(format!(
                "node offer {} carries no route attribution",
                row.node_offer_id
            )));
        }
        let ingest_request =
            bookway_runtime::grpc_service_request(user_event_pb::IngestRequest {
                user_id: row.user_id.clone(),
                events: vec![purchase_event(&row.user_id, &row.order_id, &offer.route_id)],
            })
            .map_err(|error| DeliveryError::Transient(error.to_string()))?;
        self.user_event
            .clone()
            .ingest(ingest_request)
            .await
            .map_err(|status| DeliveryError::Transient(status.to_string()))?;
        Ok(())
    }

    async fn mark_delivered(&self, order_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE purchase_event_outbox SET status='delivered',last_error=NULL,updated_at=now() WHERE order_id=$1",
        )
        .bind(order_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_dead(&self, order_id: &str, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE purchase_event_outbox SET status='dead',last_error=left($2,2000),updated_at=now() WHERE order_id=$1",
        )
        .bind(order_id)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_retry(&self, order_id: &str, error: &str) -> Result<(), sqlx::Error> {
        // Same backoff shape as the Kafka domain-event relay: double the wait
        // per attempt, cap at five minutes, dead-letter after ten.
        sqlx::query(
            "UPDATE purchase_event_outbox SET status=CASE WHEN attempts >= 10 THEN 'dead' ELSE 'pending' END,available_at=now()+make_interval(secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))),last_error=left($2,2000),updated_at=now() WHERE order_id=$1",
        )
        .bind(order_id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// The event shape and idempotency key are pinned by the recommendation side:
// deterministic UUIDv5 over "bookway:contextual-purchase:{user}:{order}", so
// replays of the same order are absorbed by user-event instead of counted.
fn purchase_event(user_id: &str, order_id: &str, route_id: &str) -> user_event_pb::Event {
    let occurred_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();
    let stable_key = format!("bookway:contextual-purchase:{user_id}:{order_id}");
    user_event_pb::Event {
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
    }
}
