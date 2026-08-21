use std::sync::Arc;

use async_trait::async_trait;
use bookway_recommend_main_api::pb::{
    self as recommend_pb, recommend_main_client::RecommendMainClient,
};
use bookway_search_main_api::pb::{self as search_pb, search_main_client::SearchMainClient};
use redis::AsyncCommands;
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tonic::transport::Channel;
use uuid::Uuid;

use super::datasource::{
    AcceptedEvent, MemoryEventDao, PostgresEventDao, DaoError,
    SharedEventDao,
};
use crate::{api::pb, conf::Config};

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
    Dao(#[from] DaoError),
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
    Dao: SharedEventDao,
    feature_cache: Option<SharedFeatureCacheInvalidator>,
    recommend_main: Option<RecommendMainClient<Channel>>,
    search_main: Option<SearchMainClient<Channel>>,
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) events: UserEventService,
}

impl Domain {
    pub(crate) async fn new(
        config: Config,
        recommend_main: RecommendMainClient<Channel>,
        search_main: SearchMainClient<Channel>,
    ) -> Result<Self, bookway_data::DataError> {
        let Dao: SharedEventDao = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryEventDao::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresEventDao::new(
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
            events: UserEventService::with_clients(
                Dao,
                feature_cache,
                Some(recommend_main),
                Some(search_main),
            ),
        })
    }
}

impl UserEventService {
    #[cfg(test)]
    pub(crate) fn with_feature_cache(
        Dao: SharedEventDao,
        feature_cache: Option<SharedFeatureCacheInvalidator>,
    ) -> Self {
        Self::with_feature_cache_and_recommend_main(Dao, feature_cache, None)
    }

    #[cfg(test)]
    pub(crate) fn with_feature_cache_and_recommend_main(
        Dao: SharedEventDao,
        feature_cache: Option<SharedFeatureCacheInvalidator>,
        recommend_main: Option<RecommendMainClient<Channel>>,
    ) -> Self {
        Self::with_clients(Dao, feature_cache, recommend_main, None)
    }

    pub(crate) fn with_clients(
        Dao: SharedEventDao,
        feature_cache: Option<SharedFeatureCacheInvalidator>,
        recommend_main: Option<RecommendMainClient<Channel>>,
        search_main: Option<SearchMainClient<Channel>>,
    ) -> Self {
        Self {
            Dao,
            feature_cache,
            recommend_main,
            search_main,
        }
    }

    pub(crate) async fn ingest(
        &self,
        request: pb::IngestRequest,
    ) -> Result<pb::IngestResponse, IngestError> {
        let user_id = request.user_id;
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
                    user_id: user_id.clone(),
                    event,
                });
            } else {
                rejected += 1;
            }
        }

        self.validate_attribution(&user_id, &mut accepted_events, &mut rejected)
            .await;

        let stored = self.Dao.store(accepted_events).await?;
        if stored.accepted > 0 {
            self.invalidate_user_features(&user_id).await;
        }
        Ok(pb::IngestResponse {
            accepted: u64::try_from(stored.accepted).unwrap_or(u64::MAX),
            duplicate: u64::try_from(stored.duplicate).unwrap_or(u64::MAX),
            rejected: u64::try_from(rejected).unwrap_or(u64::MAX),
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

    async fn validate_attribution(
        &self,
        user_id: &str,
        events: &mut Vec<AcceptedEvent>,
        rejected: &mut usize,
    ) {
        let recommendation_indices = events
            .iter()
            .enumerate()
            .filter_map(|(index, accepted)| {
                (accepted.event.request_id.is_some()
                    && accepted.event.attribution_source != pb::AttributionSource::Search as i32)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        self.validate_recommendation_attribution(
            user_id,
            events,
            &recommendation_indices,
            rejected,
        )
        .await;
        let search_indices = events
            .iter()
            .enumerate()
            .filter_map(|(index, accepted)| {
                (accepted.event.request_id.is_some()
                    && accepted.event.attribution_source == pb::AttributionSource::Search as i32)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        self.validate_search_attribution(user_id, events, &search_indices, rejected)
            .await;
    }

    async fn validate_recommendation_attribution(
        &self,
        user_id: &str,
        events: &mut Vec<AcceptedEvent>,
        indices: &[usize],
        rejected: &mut usize,
    ) {
        if indices.is_empty() {
            return;
        }
        let Some(mut client) = self.recommend_main.clone() else {
            tracing::warn!(
                count = indices.len(),
                "recommendation attribution validation is not configured; storing events without attribution"
            );
            strip_attribution(events, indices);
            return;
        };
        let request = recommend_pb::ValidateAttributionsRequest {
            user_id: user_id.to_string(),
            attributions: indices
                .iter()
                .map(|index| attribution_from_event(&events[*index].event))
                .collect(),
        };
        let result = async {
            let request = bookway_runtime::grpc_service_request(request)
                .map_err(|error| error.to_string())?;
            let response = client
                .validate_attributions(request)
                .await
                .map_err(|error| error.to_string())?
                .into_inner();
            (response.valid.len() == indices.len())
                .then_some(response.valid)
                .ok_or_else(|| {
                    "Recommend Main returned a malformed attribution response".to_string()
                })
        }
        .await;
        match result {
            Ok(valid) => retain_verified_attributions(events, indices, valid, rejected),
            Err(error) => {
                // Feedback remains useful for online features during a transient
                // recommender outage, but cannot be trusted as training attribution.
                tracing::warn!(%error, count = indices.len(), "recommendation attribution validation degraded; storing events without attribution");
                strip_attribution(events, indices);
            }
        }
    }

    async fn validate_search_attribution(
        &self,
        user_id: &str,
        events: &mut Vec<AcceptedEvent>,
        indices: &[usize],
        rejected: &mut usize,
    ) {
        if indices.is_empty() {
            return;
        }
        let Some(mut client) = self.search_main.clone() else {
            tracing::warn!(
                count = indices.len(),
                "search attribution validation is not configured; storing events without attribution"
            );
            strip_attribution(events, indices);
            return;
        };
        let request = search_pb::ValidateSearchAttributionsRequest {
            user_id: user_id.to_string(),
            attributions: indices
                .iter()
                .map(|index| search_attribution_from_event(&events[*index].event))
                .collect(),
        };
        let result = async {
            let request = bookway_runtime::grpc_service_request(request)
                .map_err(|error| error.to_string())?;
            let response = client
                .validate_attributions(request)
                .await
                .map_err(|error| error.to_string())?
                .into_inner();
            (response.valid.len() == indices.len())
                .then_some(response.valid)
                .ok_or_else(|| "Search Main returned a malformed attribution response".to_string())
        }
        .await;
        match result {
            Ok(valid) => retain_verified_attributions(events, indices, valid, rejected),
            Err(error) => {
                tracing::warn!(%error, count = indices.len(), "search attribution validation degraded; storing events without attribution");
                strip_attribution(events, indices);
            }
        }
    }
}

fn feature_cache_key(user_id: &str) -> String {
    format!("bookway:features:{user_id}")
}

fn is_valid(event: &pb::Event) -> bool {
    valid_uuid(&event.event_id)
        && valid_event_type(&event.event_type)
        && valid_identifier(&event.session_id)
        && valid_identifier(&event.component_id)
        && valid_occurred_at(&event.occurred_at)
        && !event.source.trim().is_empty()
        && event.source.len() <= MAX_SOURCE_LENGTH
        && event.request_id.as_deref().is_none_or(valid_uuid)
        && event.position.is_none_or(|position| i32::try_from(position).is_ok())
        && pb::AttributionSource::try_from(event.attribution_source).is_ok()
        && valid_attribution_shape(event)
        && valid_negative_feedback_reason(event)
        // Content IDs are opaque domain identifiers. They may be UUIDs in
        // PostgreSQL, but memory mode and imported content use slugs as well.
        && event.content_id.as_deref().is_none_or(valid_identifier)
}

fn valid_attribution_shape(event: &pb::Event) -> bool {
    event.request_id.is_none()
        || (event.content_id.as_deref().is_some_and(valid_identifier) && event.position.is_some())
}

fn valid_negative_feedback_reason(event: &pb::Event) -> bool {
    let Some(reason) = event.negative_feedback_reason else {
        return true;
    };
    event.event_type == "hide"
        && matches!(
            pb::NegativeFeedbackReason::try_from(reason).ok(),
            Some(
                pb::NegativeFeedbackReason::NotRelevant
                    | pb::NegativeFeedbackReason::AlreadySeen
                    | pb::NegativeFeedbackReason::LowQuality
            )
        )
}

fn attribution_from_event(event: &pb::Event) -> recommend_pb::ExposureAttribution {
    recommend_pb::ExposureAttribution {
        request_id: event.request_id.clone().unwrap_or_default(),
        session_id: event.session_id.clone(),
        content_id: event.content_id.clone().unwrap_or_default(),
        position: event.position.unwrap_or_default(),
    }
}

fn search_attribution_from_event(event: &pb::Event) -> search_pb::SearchAttribution {
    search_pb::SearchAttribution {
        request_id: event.request_id.clone().unwrap_or_default(),
        session_id: event.session_id.clone(),
        result_id: event.content_id.clone().unwrap_or_default(),
        position: event.position.unwrap_or_default(),
    }
}

fn strip_attribution(events: &mut [AcceptedEvent], indices: &[usize]) {
    for index in indices {
        events[*index].event.request_id = None;
        events[*index].event.position = None;
        events[*index].event.attribution_source = pb::AttributionSource::Unspecified as i32;
    }
}

fn retain_verified_attributions(
    events: &mut Vec<AcceptedEvent>,
    indices: &[usize],
    valid: Vec<bool>,
    rejected: &mut usize,
) {
    let mut verified = vec![true; events.len()];
    for (index, is_valid) in indices.iter().zip(valid) {
        verified[*index] = is_valid;
    }
    let mut retained = Vec::with_capacity(events.len());
    for (index, event) in events.drain(..).enumerate() {
        if verified[index] {
            retained.push(event);
        } else {
            *rejected += 1;
        }
    }
    *events = retained;
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
            | "save_knowledge"
            | "share"
            | "hide"
            | "complete"
            | "join_route"
            | "follow"
            | "report"
            | "search_submit"
            | "purchase"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::api::pb;
    use async_trait::async_trait;

    use super::{
        FeatureCacheInvalidator, IngestError, UserEventService, is_valid,
        retain_verified_attributions,
    };
    use crate::datasource::{AcceptedEvent, MemoryEventDao};

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

    fn event(id: &str, event_type: &str) -> pb::Event {
        pb::Event {
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
            attribution_source: pb::AttributionSource::Unspecified as i32,
            negative_feedback_reason: None,
        }
    }

    fn request(events: Vec<pb::Event>) -> pb::IngestRequest {
        pb::IngestRequest {
            user_id: "user-1".to_string(),
            events,
        }
    }

    #[tokio::test]
    async fn counts_accepted_rejected_and_duplicate_events() {
        let service =
            UserEventService::with_feature_cache(Arc::new(MemoryEventDao::default()), None);
        let first = service
            .ingest(request(vec![
                event("01980000-0000-7000-8000-000000000001", "impression"),
                event("01980000-0000-7000-8000-000000000002", "unknown"),
            ]))
            .await
            .expect("first batch should succeed");
        assert_eq!((first.accepted, first.duplicate, first.rejected), (1, 0, 1));

        let second = service
            .ingest(request(vec![event(
                "01980000-0000-7000-8000-000000000001",
                "click",
            )]))
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
            UserEventService::with_feature_cache(Arc::new(MemoryEventDao::default()), None);
        let error = service
            .ingest(request(Vec::new()))
            .await
            .expect_err("empty batch should fail");
        assert!(matches!(error, IngestError::EmptyBatch));
    }

    #[tokio::test]
    async fn accepts_opaque_content_ids_for_feedback() {
        let service =
            UserEventService::with_feature_cache(Arc::new(MemoryEventDao::default()), None);
        let mut impression = event("01980000-0000-7000-8000-000000000003", "impression");
        impression.content_id = Some("post-reading".to_string());
        let result = service
            .ingest(request(vec![impression]))
            .await
            .expect("opaque content ids should be valid");
        assert_eq!(result.accepted, 1);
        assert_eq!(result.rejected, 0);
    }

    #[tokio::test]
    async fn invalidates_online_features_only_after_a_new_event_is_stored() {
        let cache = Arc::new(RecordingFeatureCache::default());
        let service = UserEventService::with_feature_cache(
            Arc::new(MemoryEventDao::default()),
            Some(cache.clone()),
        );
        let event = event("01980000-0000-7000-8000-000000000004", "like");

        service
            .ingest(request(vec![event.clone()]))
            .await
            .expect("new event should be stored");
        service
            .ingest(request(vec![event]))
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
            Arc::new(MemoryEventDao::default()),
            Some(cache),
        );

        let response = service
            .ingest(request(vec![event(
                "01980000-0000-7000-8000-000000000005",
                "hide",
            )]))
            .await
            .expect("event persistence should not depend on Redis");

        assert_eq!(response.accepted, 1);
    }

    #[test]
    fn accepts_mobile_conversion_and_safety_events() {
        assert!(super::valid_event_type("join_route"));
        assert!(super::valid_event_type("save_knowledge"));
        assert!(super::valid_event_type("follow"));
        assert!(super::valid_event_type("report"));
        assert!(super::valid_event_type("purchase"));
    }

    #[test]
    fn accepts_unattributed_follow_events_without_a_content_id() {
        let mut follow = event("01980000-0000-7000-8000-000000000011", "follow");
        follow.component_id = "creator-follow".to_string();
        follow.content_id = None;
        follow.request_id = None;
        follow.position = None;

        assert!(is_valid(&follow));
    }

    #[test]
    fn accepts_typed_hide_reasons_but_rejects_them_for_other_events() {
        let mut hide = event("01980000-0000-7000-8000-000000000012", "hide");
        hide.negative_feedback_reason = Some(pb::NegativeFeedbackReason::AlreadySeen as i32);
        assert!(is_valid(&hide));

        let mut like = event("01980000-0000-7000-8000-000000000013", "like");
        like.negative_feedback_reason = Some(pb::NegativeFeedbackReason::NotRelevant as i32);
        assert!(!is_valid(&like));

        let mut unspecified = event("01980000-0000-7000-8000-000000000014", "hide");
        unspecified.negative_feedback_reason = Some(pb::NegativeFeedbackReason::Unspecified as i32);
        assert!(!is_valid(&unspecified));
    }

    #[test]
    fn request_attribution_requires_content_and_position() {
        let mut missing_content = event("01980000-0000-7000-8000-000000000006", "click");
        missing_content.content_id = None;
        assert!(!is_valid(&missing_content));

        let mut missing_position = event("01980000-0000-7000-8000-000000000007", "click");
        missing_position.position = None;
        assert!(!is_valid(&missing_position));
    }

    #[test]
    fn invalid_attribution_is_rejected_without_dropping_unattributed_feedback() {
        let attributed = event("01980000-0000-7000-8000-000000000008", "hide");
        let mut unattributed = event("01980000-0000-7000-8000-000000000009", "bookmark");
        unattributed.request_id = None;
        unattributed.position = None;
        let mut events = vec![
            AcceptedEvent {
                user_id: "user-1".to_string(),
                event: attributed,
            },
            AcceptedEvent {
                user_id: "user-1".to_string(),
                event: unattributed,
            },
        ];
        let mut rejected = 0;

        retain_verified_attributions(&mut events, &[0], vec![false], &mut rejected);

        assert_eq!(rejected, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.event_type, "bookmark");
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
