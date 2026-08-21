use super::*;
use time::OffsetDateTime;

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

impl CreatorRow {
    fn into_profile(self) -> Result<pb::CreatorProfile, DaoError> {
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

pub(crate) struct PostgresCreatorDao {
    pool: sqlx::PgPool,
}

impl PostgresCreatorDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CreatorDao for PostgresCreatorDao {
    async fn get(&self, user_id: &str) -> Result<pb::CreatorProfile, DaoError> {
        let row = sqlx::query_as::<_, CreatorRow>(
            "SELECT user_id,handle,headline,introduction,cover_url,specialties,featured_content_ids,state,created_at,updated_at FROM creator_profiles WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::NotFound(user_id.to_string()))?;
        row.into_profile()
    }

    async fn upsert(&self, input: CreatorProfileInput) -> Result<pb::CreatorProfile, DaoError> {
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
            Err(error) if is_unique_violation(&error) => Err(DaoError::HandleTaken(input.handle)),
            Err(error) => Err(DaoError::Database(error)),
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
    ) -> Result<Vec<pb::CreatorProfile>, DaoError> {
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
        .map_err(DaoError::Database)?;
        rows.into_iter().map(CreatorRow::into_profile).collect()
    }
}
