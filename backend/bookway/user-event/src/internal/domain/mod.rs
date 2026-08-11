use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use super::{
    api::{UserEventBatchRequest, UserEventDto, UserEventIngestResponse},
    datasource::{AcceptedEvent, RepositoryError, SharedEventRepository},
};

const MAX_BATCH_SIZE: usize = 100;
const MAX_IDENTIFIER_LENGTH: usize = 128;
const MAX_SOURCE_LENGTH: usize = 64;

#[derive(Debug, Error)]
pub(crate) enum IngestError {
    #[error("event batch must not be empty")]
    EmptyBatch,
    #[error("event batch exceeds the limit of {MAX_BATCH_SIZE}")]
    BatchTooLarge,
    #[error("trusted user identity is required")]
    MissingUser,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct UserEventService {
    repository: SharedEventRepository,
}

impl UserEventService {
    pub(crate) fn new(repository: SharedEventRepository) -> Self {
        Self { repository }
    }

    pub(crate) async fn ingest(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, IngestError> {
        if user_id.trim().is_empty() {
            return Err(IngestError::MissingUser);
        }
        if request.events.is_empty() {
            return Err(IngestError::EmptyBatch);
        }
        if request.events.len() > MAX_BATCH_SIZE {
            return Err(IngestError::BatchTooLarge);
        }

        let mut accepted_events = Vec::with_capacity(request.events.len());
        let mut rejected = 0;
        for event in request.events {
            if is_valid(&event) {
                accepted_events.push(AcceptedEvent {
                    user_id: user_id.to_string(),
                    event,
                });
            } else {
                rejected += 1;
            }
        }

        let stored = self.repository.store(accepted_events).await?;
        Ok(UserEventIngestResponse {
            accepted: stored.accepted,
            duplicate: stored.duplicate,
            rejected,
        })
    }
}

fn is_valid(event: &UserEventDto) -> bool {
    valid_uuid(&event.event_id)
        && valid_event_type(&event.event_type)
        && valid_identifier(&event.session_id)
        && valid_identifier(&event.component_id)
        && valid_occurred_at(&event.occurred_at)
        && !event.source.trim().is_empty()
        && event.source.len() <= MAX_SOURCE_LENGTH
        && event.request_id.as_deref().is_none_or(valid_uuid)
        && event.content_id.as_deref().is_none_or(valid_uuid)
}

fn valid_identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_IDENTIFIER_LENGTH
}

fn valid_occurred_at(value: &str) -> bool {
    OffsetDateTime::parse(value.trim(), &Rfc3339).is_ok()
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

fn valid_event_type(value: &str) -> bool {
    matches!(
        value,
        "impression"
            | "click"
            | "view"
            | "like"
            | "bookmark"
            | "share"
            | "hide"
            | "complete"
            | "follow"
            | "search_submit"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bookway_api::{UserEventBatchRequest, UserEventDto};

    use super::{IngestError, UserEventService};
    use crate::internal::datasource::MemoryEventRepository;

    fn event(id: &str, event_type: &str) -> UserEventDto {
        UserEventDto {
            event_id: id.to_string(),
            event_type: event_type.to_string(),
            session_id: "session-1".to_string(),
            request_id: Some("01980000-0000-7000-8000-000000000010".to_string()),
            component_id: "home-card".to_string(),
            content_id: Some("01980000-0000-7000-8000-000000000020".to_string()),
            position: Some(0),
            occurred_at: "2026-08-11T10:00:00Z".to_string(),
            source: "ios".to_string(),
        }
    }

    #[tokio::test]
    async fn counts_accepted_rejected_and_duplicate_events() {
        let service = UserEventService::new(Arc::new(MemoryEventRepository::default()));
        let first = service
            .ingest(
                "user-1",
                UserEventBatchRequest {
                    events: vec![
                        event("01980000-0000-7000-8000-000000000001", "impression"),
                        event("01980000-0000-7000-8000-000000000002", "unknown"),
                    ],
                },
            )
            .await
            .expect("first batch should succeed");
        assert_eq!((first.accepted, first.duplicate, first.rejected), (1, 0, 1));

        let second = service
            .ingest(
                "user-1",
                UserEventBatchRequest {
                    events: vec![event("01980000-0000-7000-8000-000000000001", "click")],
                },
            )
            .await
            .expect("duplicate batch should succeed");
        assert_eq!(
            (second.accepted, second.duplicate, second.rejected),
            (0, 1, 0)
        );
    }

    #[tokio::test]
    async fn rejects_empty_batches() {
        let service = UserEventService::new(Arc::new(MemoryEventRepository::default()));
        let error = service
            .ingest("user-1", UserEventBatchRequest { events: Vec::new() })
            .await
            .expect_err("empty batch should fail");
        assert!(matches!(error, IngestError::EmptyBatch));
    }
}
