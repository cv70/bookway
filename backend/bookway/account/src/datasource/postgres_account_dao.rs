use async_trait::async_trait;
use time::OffsetDateTime;

use crate::api::pb;

use super::{
    AccountDao, DaoError,
    account_profile::{default_profile, timestamp},
};

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

#[derive(Clone)]
pub(crate) struct PostgresAccountDao {
    pool: sqlx::PgPool,
}

impl PostgresAccountDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    async fn find(&self, user_id: &str) -> Result<pb::AccountProfile, DaoError> {
        let row = sqlx::query_as::<_, AccountProfileRow>(
            "SELECT user_id, display_name, avatar_url, bio, created_at, updated_at FROM account_profiles WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        row.map(AccountProfileRow::into_profile)
            .ok_or_else(|| DaoError::NotFound(user_id.to_string()))
    }
}

#[async_trait]
impl AccountDao for PostgresAccountDao {
    async fn get_or_create(&self, user_id: &str) -> Result<pb::AccountProfile, DaoError> {
        let default = default_profile(user_id);
        sqlx::query(
            "INSERT INTO account_profiles (user_id, display_name) VALUES ($1, $2) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(default.display_name)
        .execute(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        self.find(user_id).await
    }

    async fn update(
        &self,
        user_id: &str,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, DaoError> {
        let default = default_profile(user_id);
        sqlx::query(
            "INSERT INTO account_profiles (user_id, display_name) VALUES ($1, $2) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(default.display_name)
        .execute(&self.pool)
        .await
        .map_err(DaoError::Database)?;
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
        .map_err(DaoError::Database)?;
        Ok(row.into_profile())
    }
}
