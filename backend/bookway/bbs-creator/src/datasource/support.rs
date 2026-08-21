use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("creator {0} was not found")]
    NotFound(String),
    #[error("creator handle {0} is already in use")]
    HandleTaken(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored creator has an invalid state: {0}")]
    InvalidState(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreatorCursor {
    pub(crate) updated_at: OffsetDateTime,
    pub(crate) user_id: String,
}

impl CreatorCursor {
    pub(crate) fn decode(value: &str) -> Option<Self> {
        let (updated_at, user_id) = value.split_once('|')?;
        let updated_at = OffsetDateTime::parse(updated_at, &Rfc3339).ok()?;
        (!user_id.is_empty()).then(|| Self {
            updated_at,
            user_id: user_id.to_string(),
        })
    }

    pub(crate) fn from_profile(profile: &pb::CreatorProfile) -> Option<Self> {
        Some(Self {
            updated_at: OffsetDateTime::parse(&profile.updated_at, &Rfc3339).ok()?,
            user_id: profile.user_id.clone(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.updated_at), self.user_id)
    }
}

pub(crate) struct CreatorProfileInput {
    pub(crate) user_id: String,
    pub(crate) handle: String,
    pub(crate) headline: String,
    pub(crate) introduction: String,
    pub(crate) cover_url: String,
    pub(crate) specialties: Vec<String>,
    pub(crate) featured_content_ids: Vec<String>,
    pub(crate) state: i32,
}

#[async_trait]
pub(crate) trait CreatorDao: Send + Sync {
    async fn get(&self, user_id: &str) -> Result<pb::CreatorProfile, DaoError>;
    async fn upsert(&self, input: CreatorProfileInput) -> Result<pb::CreatorProfile, DaoError>;
    async fn list(
        &self,
        user_ids: &[String],
        excluded_user_ids: &[String],
        query: Option<&str>,
        specialty: Option<&str>,
        cursor: Option<&CreatorCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CreatorProfile>, DaoError>;
}

fn state_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::CreatorState::try_from(value) {
        Ok(pb::CreatorState::Active) => Ok("active"),
        Ok(pb::CreatorState::Paused) => Ok("paused"),
        Err(_) => Err(DaoError::InvalidState(value.to_string())),
    }
}

fn parse_state(value: &str) -> Result<i32, DaoError> {
    match value {
        "active" => Ok(pb::CreatorState::Active as i32),
        "paused" => Ok(pb::CreatorState::Paused as i32),
        value => Err(DaoError::InvalidState(value.to_string())),
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
}

fn now_timestamp() -> String {
    format_timestamp(OffsetDateTime::now_utc())
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[path = "memory_creator_dao.rs"]
mod memory_creator_dao;
pub(crate) use memory_creator_dao::MemoryCreatorDao;
#[path = "postgres_creator_dao.rs"]
mod postgres_creator_dao;
pub(crate) use postgres_creator_dao::PostgresCreatorDao;
