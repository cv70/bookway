use super::*;

pub(crate) struct PostgresEventDao {
    pool: sqlx::PgPool,
}

impl PostgresEventDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventDao for PostgresEventDao {
    async fn store(&self, events: Vec<AcceptedEvent>) -> Result<StoreResult, DaoError> {
        let mut tx = self.pool.begin().await.map_err(DaoError::Database)?;
        let mut result = StoreResult::default();
        for accepted in events {
            let event = &accepted.event;
            let inserted = sqlx::query_scalar::<_, String>(
                "INSERT INTO user_events (event_id,user_id,event_type,session_id,request_id,component_id,content_id,position,occurred_at,source,negative_feedback_reason) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::text::timestamptz,$10,$11) ON CONFLICT (event_id) DO NOTHING RETURNING event_id",
            )
            .bind(&event.event_id).bind(&accepted.user_id).bind(&event.event_type)
            .bind(&event.session_id).bind(&event.request_id).bind(&event.component_id)
            .bind(&event.content_id).bind(event.position.and_then(|value| i32::try_from(value).ok())).bind(&event.occurred_at).bind(&event.source).bind(negative_feedback_reason_label(event.negative_feedback_reason))
            .fetch_optional(&mut *tx).await.map_err(DaoError::Database)?;
            if inserted.is_some() {
                let payload = serde_json::json!({ "user_id": accepted.user_id, "event": event });
                sqlx::query("INSERT INTO outbox_events (aggregate_type,aggregate_id,event_type,payload) VALUES ('user_event',$1,$2,$3)")
                    .bind(&event.event_id).bind(format!("user_event.{}", event.event_type)).bind(payload)
                    .execute(&mut *tx).await.map_err(DaoError::Database)?;
                result.accepted += 1;
            } else {
                result.duplicate += 1;
            }
        }
        tx.commit().await.map_err(DaoError::Database)?;
        Ok(result)
    }
}
