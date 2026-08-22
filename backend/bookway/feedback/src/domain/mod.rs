use std::sync::Arc;

use crate::api::pb;
use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{DaoError, FeedbackDao},
};

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 100;
const MAX_CONTENT_LENGTH: usize = 2_000;
const MAX_CONTACT_LENGTH: usize = 200;
const MAX_CONTEXT_LENGTH: usize = 64;
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 128;
const MAX_RESOLUTION_LENGTH: usize = 2_000;

#[derive(Debug, Error)]
pub(crate) enum FeedbackError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] DaoError),
}

#[derive(Clone)]
pub struct Domain {
    config: Config,
    dao: Arc<FeedbackDao>,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let pool = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Postgres => Some(bookway_data::postgres_pool().await?),
            bookway_data::StorageMode::Memory => None,
        };
        Ok(Self {
            config,
            dao: Arc::new(FeedbackDao::new(pool)),
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) async fn create(
        &self,
        request: pb::CreateFeedbackRequest,
    ) -> Result<pb::FeedbackItem, FeedbackError> {
        validate_user_id(&request.user_id)?;
        let request = normalize_create(request)?;
        let idempotency_key = normalize_idempotency_key(request.idempotency_key.clone())?;
        let timestamp = now();
        let feedback = pb::FeedbackItem {
            id: uuid::Uuid::now_v7().to_string(),
            user_id: request.user_id.trim().to_string(),
            category: request.category,
            content: request.content,
            contact: request.contact,
            platform: request.platform,
            app_version: request.app_version,
            status: pb::FeedbackStatus::Pending as i32,
            resolution: None,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };
        let stored = self.dao.create(feedback.clone(), idempotency_key).await?;
        if stored.user_id != feedback.user_id
            || stored.category != feedback.category
            || stored.content != feedback.content
            || stored.contact != feedback.contact
            || stored.platform != feedback.platform
            || stored.app_version != feedback.app_version
        {
            return Err(FeedbackError::Validation(
                "idempotency key was already used for different feedback".to_string(),
            ));
        }
        Ok(stored)
    }

    pub(crate) async fn list_own(
        &self,
        request: pb::ListOwnFeedbackRequest,
    ) -> Result<pb::FeedbackList, FeedbackError> {
        validate_user_id(&request.user_id)?;
        self.dao
            .list(
                Some(request.user_id.trim()),
                parse_status(request.status)?,
                page_limit(request.limit.map(|value| value as usize))?,
            )
            .await
            .map(|items| pb::FeedbackList { items })
            .map_err(FeedbackError::from)
    }

    pub(crate) async fn list(
        &self,
        request: pb::ListFeedbackRequest,
    ) -> Result<pb::FeedbackList, FeedbackError> {
        self.dao
            .list(
                None,
                parse_status(request.status)?,
                page_limit(request.limit.map(|value| value as usize))?,
            )
            .await
            .map(|items| pb::FeedbackList { items })
            .map_err(FeedbackError::from)
    }

    pub(crate) async fn review(
        &self,
        request: pb::ReviewFeedbackRequest,
    ) -> Result<pb::FeedbackItem, FeedbackError> {
        validate_user_id(&request.reviewer_id)?;
        if request.feedback_id.trim().is_empty() {
            return Err(FeedbackError::Validation(
                "feedback_id is required".to_string(),
            ));
        }
        let resolution = request.resolution.unwrap_or_default().trim().to_string();
        if resolution.chars().count() > MAX_RESOLUTION_LENGTH {
            return Err(FeedbackError::Validation(format!(
                "resolution exceeds {MAX_RESOLUTION_LENGTH} characters"
            )));
        }
        self.dao
            .review(
                request.feedback_id.trim(),
                required_status(request.status)?,
                (!resolution.is_empty()).then_some(resolution),
            )
            .await
            .map_err(FeedbackError::from)
    }
}

fn normalize_create(
    request: pb::CreateFeedbackRequest,
) -> Result<pb::CreateFeedbackRequest, FeedbackError> {
    let content = request.content.trim().to_string();
    let contact = request.contact.trim().to_string();
    let platform = request.platform.trim().to_string();
    let app_version = request.app_version.trim().to_string();
    if content.is_empty() {
        return Err(FeedbackError::Validation(
            "feedback content is required".to_string(),
        ));
    }
    if content.chars().count() > MAX_CONTENT_LENGTH {
        return Err(FeedbackError::Validation(format!(
            "feedback content exceeds {MAX_CONTENT_LENGTH} characters"
        )));
    }
    if contact.chars().count() > MAX_CONTACT_LENGTH {
        return Err(FeedbackError::Validation(format!(
            "contact exceeds {MAX_CONTACT_LENGTH} characters"
        )));
    }
    if platform.chars().count() > MAX_CONTEXT_LENGTH
        || app_version.chars().count() > MAX_CONTEXT_LENGTH
    {
        return Err(FeedbackError::Validation(format!(
            "platform and app_version must be at most {MAX_CONTEXT_LENGTH} characters"
        )));
    }
    required_category(request.category)?;
    Ok(pb::CreateFeedbackRequest {
        user_id: request.user_id,
        idempotency_key: request.idempotency_key,
        category: request.category,
        content,
        contact,
        platform,
        app_version,
    })
}

fn validate_user_id(user_id: &str) -> Result<(), FeedbackError> {
    if user_id.trim().is_empty() || user_id.chars().count() > 256 {
        return Err(FeedbackError::Validation("invalid user_id".to_string()));
    }
    Ok(())
}

fn normalize_idempotency_key(value: Option<String>) -> Result<Option<String>, FeedbackError> {
    let value = value.map(|value| value.trim().to_string());
    if value
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.chars().count() > MAX_IDEMPOTENCY_KEY_LENGTH)
    {
        return Err(FeedbackError::Validation(
            "invalid idempotency key".to_string(),
        ));
    }
    Ok(value)
}

fn page_limit(value: Option<usize>) -> Result<usize, FeedbackError> {
    let limit = value.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(FeedbackError::Validation(format!(
            "feedback limit must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    Ok(limit)
}

fn required_category(value: i32) -> Result<pb::FeedbackCategory, FeedbackError> {
    pb::FeedbackCategory::try_from(value)
        .map_err(|_| FeedbackError::Validation("invalid feedback category".to_string()))
}

fn required_status(value: i32) -> Result<pb::FeedbackStatus, FeedbackError> {
    pb::FeedbackStatus::try_from(value)
        .map_err(|_| FeedbackError::Validation("invalid feedback status".to_string()))
}

fn parse_status(value: Option<i32>) -> Result<Option<i32>, FeedbackError> {
    value
        .map(required_status)
        .transpose()
        .map(|status| status.map(|status| status as i32))
}

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn domain() -> Domain {
        Domain {
            config: Config {
                listen_addr: "127.0.0.1:8104"
                    .parse::<SocketAddr>()
                    .expect("valid socket address"),
            },
            dao: Arc::new(FeedbackDao::new(None)),
        }
    }

    fn request(content: &str) -> pb::CreateFeedbackRequest {
        pb::CreateFeedbackRequest {
            user_id: "user-1".to_string(),
            idempotency_key: None,
            category: pb::FeedbackCategory::Bug as i32,
            content: content.to_string(),
            contact: String::new(),
            platform: "ios".to_string(),
            app_version: "1.0.0".to_string(),
        }
    }

    #[tokio::test]
    async fn duplicate_submission_replays_the_original_feedback() {
        let domain = domain();
        let first = domain
            .create(pb::CreateFeedbackRequest {
                idempotency_key: Some("feedback-1".to_string()),
                ..request("按钮无法点击")
            })
            .await
            .expect("first feedback is accepted");
        let repeated = domain
            .create(pb::CreateFeedbackRequest {
                idempotency_key: Some("feedback-1".to_string()),
                ..request("按钮无法点击")
            })
            .await
            .expect("same feedback is replayed");

        assert_eq!(first.id, repeated.id);
        assert_eq!(
            domain
                .list_own(pb::ListOwnFeedbackRequest {
                    user_id: "user-1".to_string(),
                    status: None,
                    limit: None,
                })
                .await
                .expect("history is readable")
                .items
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn feedback_requires_non_empty_content() {
        let error = domain()
            .create(request("  "))
            .await
            .expect_err("blank feedback must not be accepted");

        assert!(error.to_string().contains("content is required"));
    }
}
