use std::collections::HashMap;

use crate::api::pb;
use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("account profile {0} was not found")]
    NotFound(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[async_trait]
pub(crate) trait AccountRepository: Send + Sync {
    async fn get_or_create(&self, user_id: &str) -> Result<pb::AccountProfile, RepositoryError>;
    async fn update(
        &self,
        user_id: &str,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, RepositoryError>;
}

#[derive(Default)]
pub(crate) struct MemoryAccountRepository {
    profiles: RwLock<HashMap<String, pb::AccountProfile>>,
}

#[async_trait]
impl AccountRepository for MemoryAccountRepository {
    async fn get_or_create(&self, user_id: &str) -> Result<pb::AccountProfile, RepositoryError> {
        let mut profiles = self.profiles.write().await;
        Ok(profiles
            .entry(user_id.to_string())
            .or_insert_with(|| default_profile(user_id))
            .clone())
    }

    async fn update(
        &self,
        user_id: &str,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, RepositoryError> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .entry(user_id.to_string())
            .or_insert_with(|| default_profile(user_id));
        if let Some(display_name) = request.display_name {
            profile.display_name = display_name;
        }
        if let Some(avatar_url) = request.avatar_url {
            profile.avatar_url = avatar_url;
        }
        if let Some(bio) = request.bio {
            profile.bio = bio;
        }
        profile.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(profile.clone())
    }
}

#[derive(Clone)]
pub(crate) struct PostgresAccountRepository {
    pool: sqlx::PgPool,
}

impl PostgresAccountRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    async fn find(&self, user_id: &str) -> Result<pb::AccountProfile, RepositoryError> {
        let row = sqlx::query_as::<_, AccountProfileRow>(
            "SELECT user_id, display_name, avatar_url, bio, created_at, updated_at FROM account_profiles WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        row.map(AccountProfileRow::into_profile)
            .ok_or_else(|| RepositoryError::NotFound(user_id.to_string()))
    }
}

#[async_trait]
impl AccountRepository for PostgresAccountRepository {
    async fn get_or_create(&self, user_id: &str) -> Result<pb::AccountProfile, RepositoryError> {
        let default = default_profile(user_id);
        sqlx::query(
            "INSERT INTO account_profiles (user_id, display_name) VALUES ($1, $2) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(default.display_name)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        self.find(user_id).await
    }

    async fn update(
        &self,
        user_id: &str,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, RepositoryError> {
        let default = default_profile(user_id);
        sqlx::query(
            "INSERT INTO account_profiles (user_id, display_name) VALUES ($1, $2) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(default.display_name)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let row = sqlx::query_as::<_, AccountProfileRow>(
            r#"
            UPDATE account_profiles
            SET display_name = COALESCE($2, display_name),
                avatar_url = COALESCE($3, avatar_url),
                bio = COALESCE($4, bio),
                updated_at = now()
            WHERE user_id = $1
            RETURNING user_id, display_name, avatar_url, bio, created_at, updated_at
            "#,
        )
        .bind(user_id)
        .bind(request.display_name)
        .bind(request.avatar_url)
        .bind(request.bio)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(row.into_profile())
    }
}

#[derive(sqlx::FromRow)]
struct AccountProfileRow {
    user_id: String,
    display_name: String,
    avatar_url: String,
    bio: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl AccountProfileRow {
    fn into_profile(self) -> pb::AccountProfile {
        pb::AccountProfile {
            user_id: self.user_id,
            display_name: self.display_name,
            avatar_url: self.avatar_url,
            bio: self.bio,
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        }
    }
}

fn default_profile(user_id: &str) -> pb::AccountProfile {
    let now = timestamp(OffsetDateTime::now_utc());
    pb::AccountProfile {
        user_id: user_id.to_string(),
        display_name: if user_id == "demo-user" {
            "行路人"
        } else {
            "新行者"
        }
        .to_string(),
        avatar_url: String::new(),
        bio: String::new(),
        created_at: now.clone(),
        updated_at: now,
    }
}

fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
