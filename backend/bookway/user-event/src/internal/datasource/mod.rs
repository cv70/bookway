use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use bookway_api::UserEventDto;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub(crate) struct AcceptedEvent {
    pub(crate) user_id: String,
    pub(crate) event: UserEventDto,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoreResult {
    pub(crate) accepted: usize,
    pub(crate) duplicate: usize,
}

#[async_trait]
pub(crate) trait EventRepository: Send + Sync {
    async fn store(&self, events: Vec<AcceptedEvent>) -> Result<StoreResult, RepositoryError>;
}

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

pub(crate) type SharedEventRepository = Arc<dyn EventRepository>;

#[derive(Default)]
pub(crate) struct MemoryEventRepository {
    events: Mutex<HashMap<String, (String, UserEventDto)>>,
}

#[async_trait]
impl EventRepository for MemoryEventRepository {
    async fn store(&self, events: Vec<AcceptedEvent>) -> Result<StoreResult, RepositoryError> {
        let mut stored_events = self.events.lock().await;
        let mut result = StoreResult::default();
        for accepted_event in events {
            if stored_events.contains_key(&accepted_event.event.event_id) {
                result.duplicate += 1;
                continue;
            }
            stored_events.insert(
                accepted_event.event.event_id.clone(),
                (accepted_event.user_id, accepted_event.event),
            );
            result.accepted += 1;
        }
        Ok(result)
    }
}

pub(crate) struct PostgresEventRepository {
    pool: sqlx::PgPool,
}

impl PostgresEventRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventRepository for PostgresEventRepository {
    async fn store(&self, events: Vec<AcceptedEvent>) -> Result<StoreResult, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let mut result = StoreResult::default();
        for accepted in events {
            let event = &accepted.event;
            let inserted = sqlx::query_scalar::<_, String>(
                "INSERT INTO user_events (event_id,user_id,event_type,session_id,request_id,component_id,content_id,position,occurred_at,source) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::text::timestamptz,$10) ON CONFLICT (event_id) DO NOTHING RETURNING event_id",
            )
            .bind(&event.event_id).bind(&accepted.user_id).bind(&event.event_type)
            .bind(&event.session_id).bind(&event.request_id).bind(&event.component_id)
            .bind(&event.content_id).bind(event.position.and_then(|value| i32::try_from(value).ok())).bind(&event.occurred_at).bind(&event.source)
            .fetch_optional(&mut *tx).await.map_err(RepositoryError::Database)?;
            if inserted.is_some() {
                let payload = serde_json::json!({ "user_id": accepted.user_id, "event": event });
                sqlx::query("INSERT INTO outbox_events (aggregate_type,aggregate_id,event_type,payload) VALUES ('user_event',$1,$2,$3)")
                    .bind(&event.event_id).bind(format!("user_event.{}", event.event_type)).bind(payload)
                    .execute(&mut *tx).await.map_err(RepositoryError::Database)?;
                result.accepted += 1;
            } else {
                result.duplicate += 1;
            }
        }
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(result)
    }
}
