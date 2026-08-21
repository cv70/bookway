use std::{collections::HashMap, sync::Arc};

use crate::api::pb;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("feedback {0} was not found")]
    NotFound(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored feedback is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
}

#[derive(Default)]
struct MemoryFeedback {
    by_id: HashMap<String, pb::FeedbackItem>,
    idempotency: HashMap<(String, String), String>,
}

#[path = "feedback_dao.rs"]
mod feedback_dao;
pub(crate) use feedback_dao::FeedbackDao;
