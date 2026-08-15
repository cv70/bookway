use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
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
pub(crate) trait CreatorRepository: Send + Sync {
    async fn get(&self, user_id: &str) -> Result<pb::CreatorProfile, RepositoryError>;
    async fn upsert(
        &self,
        input: CreatorProfileInput,
    ) -> Result<pb::CreatorProfile, RepositoryError>;
    async fn list(
        &self,
        user_ids: &[String],
        excluded_user_ids: &[String],
        query: Option<&str>,
        specialty: Option<&str>,
        cursor: Option<&CreatorCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CreatorProfile>, RepositoryError>;
}

#[derive(Default)]
pub(crate) struct MemoryCreatorRepository {
    profiles: RwLock<HashMap<String, pb::CreatorProfile>>,
}

#[async_trait]
impl CreatorRepository for MemoryCreatorRepository {
    async fn get(&self, user_id: &str) -> Result<pb::CreatorProfile, RepositoryError> {
        self.profiles
            .read()
            .await
            .get(user_id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(user_id.to_string()))
    }

    async fn upsert(
        &self,
        input: CreatorProfileInput,
    ) -> Result<pb::CreatorProfile, RepositoryError> {
        let mut profiles = self.profiles.write().await;
        if profiles
            .values()
            .any(|profile| profile.user_id != input.user_id && profile.handle == input.handle)
        {
            return Err(RepositoryError::HandleTaken(input.handle));
        }
        let now = now_timestamp();
        let created_at = profiles
            .get(&input.user_id)
            .map(|profile| profile.created_at.clone())
            .unwrap_or_else(|| now.clone());
        let profile = pb::CreatorProfile {
            user_id: input.user_id.clone(),
            handle: input.handle,
            headline: input.headline,
            introduction: input.introduction,
            cover_url: input.cover_url,
            specialties: input.specialties,
            featured_content_ids: input.featured_content_ids,
            state: input.state,
            created_at,
            updated_at: now,
        };
        profiles.insert(input.user_id, profile.clone());
        Ok(profile)
    }

    async fn list(
        &self,
        user_ids: &[String],
        excluded_user_ids: &[String],
        query: Option<&str>,
        specialty: Option<&str>,
        cursor: Option<&CreatorCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CreatorProfile>, RepositoryError> {
        let query = query.map(str::to_lowercase);
        let specialty = specialty.map(str::to_lowercase);
        let mut profiles = self
            .profiles
            .read()
            .await
            .values()
            .filter(|profile| user_ids.is_empty() || user_ids.contains(&profile.user_id))
            .filter(|profile| !excluded_user_ids.contains(&profile.user_id))
            .filter(|profile| {
                !user_ids.is_empty() || profile.state == pb::CreatorState::Active as i32
            })
            .filter(|profile| {
                query.as_ref().is_none_or(|query| {
                    [
                        profile.handle.as_str(),
                        profile.headline.as_str(),
                        profile.introduction.as_str(),
                    ]
                    .into_iter()
                    .any(|field| field.to_lowercase().contains(query))
                })
            })
            .filter(|profile| {
                specialty.as_ref().is_none_or(|specialty| {
                    profile
                        .specialties
                        .iter()
                        .any(|value| value.to_lowercase() == *specialty)
                })
            })
            .filter(|profile| {
                cursor.is_none_or(|cursor| {
                    OffsetDateTime::parse(&profile.updated_at, &Rfc3339).is_ok_and(|updated_at| {
                        (updated_at, profile.user_id.as_str())
                            < (cursor.updated_at, cursor.user_id.as_str())
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| {
            let left_at = OffsetDateTime::parse(&left.updated_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            let right_at = OffsetDateTime::parse(&right.updated_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            right_at
                .cmp(&left_at)
                .then_with(|| right.user_id.cmp(&left.user_id))
        });
        profiles.truncate(limit);
        Ok(profiles)
    }
}

pub(crate) struct PostgresCreatorRepository {
    pool: sqlx::PgPool,
}

impl PostgresCreatorRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct CreatorRow {
    user_id: String,
    handle: String,
    headline: String,
    introduction: String,
    cover_url: String,
    specialties: Vec<String>,
    featured_content_ids: Vec<String>,
    state: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[async_trait]
impl CreatorRepository for PostgresCreatorRepository {
    async fn get(&self, user_id: &str) -> Result<pb::CreatorProfile, RepositoryError> {
        let row = sqlx::query_as::<_, CreatorRow>(
            "SELECT user_id,handle,headline,introduction,cover_url,specialties,featured_content_ids,state,created_at,updated_at FROM creator_profiles WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(user_id.to_string()))?;
        row.into_profile()
    }

    async fn upsert(
        &self,
        input: CreatorProfileInput,
    ) -> Result<pb::CreatorProfile, RepositoryError> {
        let state = state_name(input.state)?;
        let result = sqlx::query_as::<_, CreatorRow>(
            "INSERT INTO creator_profiles (user_id,handle,headline,introduction,cover_url,specialties,featured_content_ids,state) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (user_id) DO UPDATE SET handle = EXCLUDED.handle,headline = EXCLUDED.headline,introduction = EXCLUDED.introduction,cover_url = EXCLUDED.cover_url,specialties = EXCLUDED.specialties,featured_content_ids = EXCLUDED.featured_content_ids,state = EXCLUDED.state,updated_at = now() RETURNING user_id,handle,headline,introduction,cover_url,specialties,featured_content_ids,state,created_at,updated_at",
        )
        .bind(&input.user_id)
        .bind(&input.handle)
        .bind(&input.headline)
        .bind(&input.introduction)
        .bind(&input.cover_url)
        .bind(&input.specialties)
        .bind(&input.featured_content_ids)
        .bind(state)
        .fetch_one(&self.pool)
        .await;
        match result {
            Ok(row) => row.into_profile(),
            Err(error) if is_unique_violation(&error) => {
                Err(RepositoryError::HandleTaken(input.handle))
            }
            Err(error) => Err(RepositoryError::Database(error)),
        }
    }

    async fn list(
        &self,
        user_ids: &[String],
        excluded_user_ids: &[String],
        query: Option<&str>,
        specialty: Option<&str>,
        cursor: Option<&CreatorCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CreatorProfile>, RepositoryError> {
        let cursor_at = cursor.map(|cursor| cursor.updated_at);
        let cursor_id = cursor.map(|cursor| cursor.user_id.as_str());
        let query = query.map(|value| format!("%{value}%"));
        let rows = sqlx::query_as::<_, CreatorRow>(
            "SELECT user_id,handle,headline,introduction,cover_url,specialties,featured_content_ids,state,created_at,updated_at FROM creator_profiles WHERE (cardinality($1::TEXT[]) = 0 OR user_id = ANY($1)) AND (cardinality($2::TEXT[]) = 0 OR user_id <> ALL($2)) AND (cardinality($1::TEXT[]) > 0 OR state = 'active') AND ($3::TEXT IS NULL OR handle ILIKE $3 OR headline ILIKE $3 OR introduction ILIKE $3) AND ($4::TEXT IS NULL OR $4 = ANY(specialties)) AND ($5::TIMESTAMPTZ IS NULL OR (updated_at,user_id) < ($5,$6)) ORDER BY updated_at DESC,user_id DESC LIMIT $7",
        )
        .bind(user_ids)
        .bind(excluded_user_ids)
        .bind(query)
        .bind(specialty)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter().map(CreatorRow::into_profile).collect()
    }
}

impl CreatorRow {
    fn into_profile(self) -> Result<pb::CreatorProfile, RepositoryError> {
        Ok(pb::CreatorProfile {
            user_id: self.user_id,
            handle: self.handle,
            headline: self.headline,
            introduction: self.introduction,
            cover_url: self.cover_url,
            specialties: self.specialties,
            featured_content_ids: self.featured_content_ids,
            state: parse_state(&self.state)?,
            created_at: format_timestamp(self.created_at),
            updated_at: format_timestamp(self.updated_at),
        })
    }
}

fn state_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::CreatorState::try_from(value) {
        Ok(pb::CreatorState::Active) => Ok("active"),
        Ok(pb::CreatorState::Paused) => Ok("paused"),
        Err(_) => Err(RepositoryError::InvalidState(value.to_string())),
    }
}

fn parse_state(value: &str) -> Result<i32, RepositoryError> {
    match value {
        "active" => Ok(pb::CreatorState::Active as i32),
        "paused" => Ok(pb::CreatorState::Paused as i32),
        value => Err(RepositoryError::InvalidState(value.to_string())),
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
