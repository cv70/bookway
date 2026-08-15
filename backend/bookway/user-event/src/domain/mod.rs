use std::sync::Arc;

use async_trait::async_trait;
use redis::AsyncCommands;
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use super::{
    api::{UserEventBatchRequest, UserEventDto, UserEventIngestResponse},
    datasource::{
        AcceptedEvent, MemoryEventRepository, PostgresEventRepository, RepositoryError,
        SharedEventRepository,
    },
};
use crate::conf::Config;

const MAX_BATCH_SIZE: usize = 100;
const MAX_IDENTIFIER_LENGTH: usize = 128;
const MAX_SOURCE_LENGTH: usize = 64;
const MAX_EVENT_AGE: Duration = Duration::days(90);
const MAX_FUTURE_SKEW: Duration = Duration::minutes(5);

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

#[async_trait]
pub(crate) trait FeatureCacheInvalidator: Send + Sync {
    async fn invalidate(&self, user_id: &str) -> Result<(), String>;
}

pub(crate) type SharedFeatureCacheInvalidator = Arc<dyn FeatureCacheInvalidator>;

pub(crate) struct RedisFeatureCacheInvalidator {
    redis: redis::aio::ConnectionManager,
}

impl RedisFeatureCacheInvalidator {
    pub(crate) fn new(redis: redis::aio::ConnectionManager) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl FeatureCacheInvalidator for RedisFeatureCacheInvalidator {
    async fn invalidate(&self, user_id: &str) -> Result<(), String> {
        let mut redis = self.redis.clone();
        let _: usize = redis
            .del(feature_cache_key(user_id))
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct UserEventService {
    repository: SharedEventRepository,
    feature_cache: Option<SharedFeatureCacheInvalidator>,
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) events: UserEventService,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let repository: SharedEventRepository = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryEventRepository::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresEventRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let feature_cache = match bookway_data::redis_connection().await {
            Ok(Some(redis)) => {
                Some(Arc::new(RedisFeatureCacheInvalidator::new(redis))
                    as SharedFeatureCacheInvalidator)
            }
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, "redis unavailable at startup; user feature cache invalidation disabled");
                None
            }
        };
        Ok(Self {
            config,
            events: UserEventService::with_feature_cache(repository, feature_cache),
        })
    }
}

impl UserEventService {
    pub(crate) fn with_feature_cache(
        repository: SharedEventRepository,
        feature_cache: Option<SharedFeatureCacheInvalidator>,
    ) -> Self {
        Self {
            repository,
            feature_cache,
        }
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
        if stored.accepted > 0 {
            self.invalidate_user_features(user_id).await;
        }
        Ok(UserEventIngestResponse {
            accepted: stored.accepted,
            duplicate: stored.duplicate,
            rejected,
        })
    }

    async fn invalidate_user_features(&self, user_id: &str) {
        let Some(feature_cache) = &self.feature_cache else {
            return;
        };
        if let Err(error) = feature_cache.invalidate(user_id).await {
            // The canonical event log has committed. Recomputing after the cache TTL
            // is safe, so an invalidation outage must not turn feedback into a failure.
            tracing::warn!(%error, user_id, "user feature cache invalidation degraded");
        }
    }
}

fn feature_cache_key(user_id: &str) -> String {
    format!("bookway:features:{user_id}")
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
        // Content IDs are opaque domain identifiers. They may be UUIDs in
        // PostgreSQL, but memory mode and imported content use slugs as well.
        && event.content_id.as_deref().is_none_or(valid_identifier)
}

fn valid_identifier(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_IDENTIFIER_LENGTH
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

fn valid_occurred_at(value: &str) -> bool {
    valid_occurred_at_for(value, OffsetDateTime::now_utc())
}

fn valid_occurred_at_for(value: &str, now: OffsetDateTime) -> bool {
    OffsetDateTime::parse(value.trim(), &Rfc3339).is_ok_and(|occurred_at| {
        occurred_at >= now - MAX_EVENT_AGE && occurred_at <= now + MAX_FUTURE_SKEW
    })
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
            | "join_route"
            | "follow"
            | "report"
            | "search_submit"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use bookway_api::{UserEventBatchRequest, UserEventDto};

    use super::{FeatureCacheInvalidator, IngestError, UserEventService};
    use crate::datasource::MemoryEventRepository;

    #[derive(Default)]
    struct RecordingFeatureCache {
        invalidated_user_ids: Mutex<Vec<String>>,
        fails: bool,
    }

    #[async_trait]
    impl FeatureCacheInvalidator for RecordingFeatureCache {
        async fn invalidate(&self, user_id: &str) -> Result<(), String> {
            self.invalidated_user_ids
                .lock()
                .expect("feature cache lock")
                .push(user_id.to_string());
            if self.fails {
                return Err("cache unavailable".to_string());
            }
            Ok(())
        }
    }

    fn event(id: &str, event_type: &str) -> UserEventDto {
        UserEventDto {
            event_id: id.to_string(),
            event_type: event_type.to_string(),
            session_id: "session-1".to_string(),
            request_id: Some("01980000-0000-7000-8000-000000000010".to_string()),
            component_id: "home-card".to_string(),
            content_id: Some("01980000-0000-7000-8000-000000000020".to_string()),
            position: Some(0),
            occurred_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .expect("current timestamp"),
            source: "ios".to_string(),
        }
    }

    #[tokio::test]
    async fn counts_accepted_rejected_and_duplicate_events() {
        let service =
            UserEventService::with_feature_cache(Arc::new(MemoryEventRepository::default()), None);
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
        let service =
            UserEventService::with_feature_cache(Arc::new(MemoryEventRepository::default()), None);
        let error = service
            .ingest("user-1", UserEventBatchRequest { events: Vec::new() })
            .await
            .expect_err("empty batch should fail");
        assert!(matches!(error, IngestError::EmptyBatch));
    }

    #[tokio::test]
    async fn accepts_opaque_content_ids_for_feedback() {
        let service =
            UserEventService::with_feature_cache(Arc::new(MemoryEventRepository::default()), None);
        let mut impression = event("01980000-0000-7000-8000-000000000003", "impression");
        impression.content_id = Some("post-reading".to_string());
        let result = service
            .ingest(
                "user-1",
                UserEventBatchRequest {
                    events: vec![impression],
                },
            )
            .await
            .expect("opaque content ids should be valid");
        assert_eq!(result.accepted, 1);
        assert_eq!(result.rejected, 0);
    }

    #[tokio::test]
    async fn invalidates_online_features_only_after_a_new_event_is_stored() {
        let cache = Arc::new(RecordingFeatureCache::default());
        let service = UserEventService::with_feature_cache(
            Arc::new(MemoryEventRepository::default()),
            Some(cache.clone()),
        );
        let event = event("01980000-0000-7000-8000-000000000004", "like");

        service
            .ingest(
                "user-1",
                UserEventBatchRequest {
                    events: vec![event.clone()],
                },
            )
            .await
            .expect("new event should be stored");
        service
            .ingest(
                "user-1",
                UserEventBatchRequest {
                    events: vec![event],
                },
            )
            .await
            .expect("duplicate should be accepted without a new invalidation");

        assert_eq!(
            *cache
                .invalidated_user_ids
                .lock()
                .expect("feature cache lock"),
            ["user-1"]
        );
    }

    #[tokio::test]
    async fn cache_invalidation_failure_does_not_reject_persisted_feedback() {
        let cache = Arc::new(RecordingFeatureCache {
            invalidated_user_ids: Mutex::new(Vec::new()),
            fails: true,
        });
        let service = UserEventService::with_feature_cache(
            Arc::new(MemoryEventRepository::default()),
            Some(cache),
        );

        let response = service
            .ingest(
                "user-1",
                UserEventBatchRequest {
                    events: vec![event("01980000-0000-7000-8000-000000000005", "hide")],
                },
            )
            .await
            .expect("event persistence should not depend on Redis");

        assert_eq!(response.accepted, 1);
    }

    #[test]
    fn accepts_mobile_conversion_and_safety_events() {
        assert!(super::valid_event_type("join_route"));
        assert!(super::valid_event_type("report"));
    }

    #[test]
    fn rejects_timestamps_that_can_poison_online_features() {
        let now = time::OffsetDateTime::parse(
            "2026-08-15T10:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("fixed timestamp");

        assert!(super::valid_occurred_at_for("2026-08-15T09:59:00Z", now));
        assert!(!super::valid_occurred_at_for("2026-11-15T10:00:00Z", now));
        assert!(!super::valid_occurred_at_for("2025-08-15T10:00:00Z", now));
    }
}
