use std::{
    env,
    sync::{Arc, Mutex},
    time::Duration,
};

use kafka::producer::{Producer, Record, RequiredAcks};
use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EventError {
    #[error("KAFKA_BROKERS is required")]
    MissingBrokers,
    #[error("kafka client creation failed: {0}")]
    Kafka(#[from] kafka::Error),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug)]
struct OutboxEvent {
    id: Uuid,
    event_type: String,
    payload: Value,
}

#[derive(Clone)]
pub struct OutboxRelay {
    pool: PgPool,
    producer: Arc<Mutex<Producer>>,
    topic: String,
    batch_size: i64,
}

impl OutboxRelay {
    pub fn from_env(pool: PgPool) -> Result<Self, EventError> {
        let brokers = env::var("KAFKA_BROKERS").map_err(|_| EventError::MissingBrokers)?;
        let topic = env::var("KAFKA_OUTBOX_TOPIC")
            .unwrap_or_else(|_| "bookway.domain-events.v1".to_string());
        let batch_size = env::var("OUTBOX_BATCH_SIZE")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100)
            .clamp(1, 1000);
        let producer = Producer::from_hosts(
            brokers
                .split(',')
                .map(str::trim)
                .map(str::to_string)
                .collect(),
        )
        .with_ack_timeout(Duration::from_secs(10))
        .with_required_acks(RequiredAcks::All)
        .create()?;
        Ok(Self {
            pool,
            producer: Arc::new(Mutex::new(producer)),
            topic,
            batch_size,
        })
    }

    pub async fn run_once(&self) -> Result<usize, EventError> {
        let events = self.claim().await?;
        let count = events.len();
        for event in events {
            let payload = serde_json::to_string(
                &serde_json::json!({ "event_type": event.event_type, "data": event.payload }),
            )
            .unwrap_or_else(|_| "{}".to_string());
            let key = event.id.to_string();
            let result = match self.producer.lock() {
                Ok(mut producer) => Some(producer.send(&Record::from_key_value(
                    &self.topic,
                    key.as_bytes(),
                    payload.as_bytes(),
                ))),
                Err(error) => {
                    let message = error.to_string();
                    drop(error);
                    self.mark_failed(event.id, &message).await?;
                    None
                }
            };
            let Some(result) = result else {
                continue;
            };
            match result {
                Ok(()) => self.mark_published(event.id).await?,
                Err(error) => self.mark_failed(event.id, &error.to_string()).await?,
            }
        }
        Ok(count)
    }

    async fn claim(&self) -> Result<Vec<OutboxEvent>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (Uuid, String, Value)>(
            "WITH claimed AS (SELECT id FROM outbox_events WHERE ((status = 'pending' AND available_at <= now()) OR (status = 'publishing' AND available_at <= now() - interval '5 minutes')) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE outbox_events o SET status='publishing', attempts=attempts+1, available_at=now() FROM claimed WHERE o.id=claimed.id RETURNING o.id,o.event_type,o.payload",
        ).bind(self.batch_size).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|(id, event_type, payload)| OutboxEvent {
                id,
                event_type,
                payload,
            })
            .collect())
    }

    async fn mark_published(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE outbox_events SET status='published',published_at=now(),last_error=NULL WHERE id=$1")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE outbox_events SET status=CASE WHEN attempts >= 10 THEN 'dead' ELSE 'pending' END,available_at=now()+make_interval(secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))),last_error=left($2,2000) WHERE id=$1")
            .bind(id).bind(error).execute(&self.pool).await?;
        Ok(())
    }
}
