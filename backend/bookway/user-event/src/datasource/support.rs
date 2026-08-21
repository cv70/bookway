use std::{collections::HashMap, sync::Arc};

use crate::api::pb;
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub(crate) struct AcceptedEvent {
    pub(crate) user_id: String,
    pub(crate) event: pb::Event,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoreResult {
    pub(crate) accepted: usize,
    pub(crate) duplicate: usize,
}

#[async_trait]
pub(crate) trait EventDao: Send + Sync {
    async fn store(&self, events: Vec<AcceptedEvent>) -> Result<StoreResult, DaoError>;
}

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

pub(crate) type SharedEventDao = Arc<dyn EventDao>;

fn negative_feedback_reason_label(value: Option<i32>) -> Option<&'static str> {
    match pb::NegativeFeedbackReason::try_from(value?).ok()? {
        pb::NegativeFeedbackReason::NotRelevant => Some("not_relevant"),
        pb::NegativeFeedbackReason::AlreadySeen => Some("already_seen"),
        pb::NegativeFeedbackReason::LowQuality => Some("low_quality"),
        pb::NegativeFeedbackReason::Unspecified => None,
    }
}

#[cfg(test)]
mod tests {
    use super::negative_feedback_reason_label;
    use crate::api::pb;

    #[test]
    fn persists_only_supported_negative_feedback_reason_labels() {
        assert_eq!(
            negative_feedback_reason_label(Some(pb::NegativeFeedbackReason::NotRelevant as i32)),
            Some("not_relevant")
        );
        assert_eq!(
            negative_feedback_reason_label(Some(pb::NegativeFeedbackReason::AlreadySeen as i32)),
            Some("already_seen")
        );
        assert_eq!(
            negative_feedback_reason_label(Some(pb::NegativeFeedbackReason::LowQuality as i32)),
            Some("low_quality")
        );
        assert_eq!(
            negative_feedback_reason_label(Some(pb::NegativeFeedbackReason::Unspecified as i32)),
            None
        );
        assert_eq!(negative_feedback_reason_label(Some(99)), None);
    }
}

#[path = "memory_event_dao.rs"]
mod memory_event_dao;
pub(crate) use memory_event_dao::MemoryEventDao;
#[path = "postgres_event_dao.rs"]
mod postgres_event_dao;
pub(crate) use postgres_event_dao::PostgresEventDao;
