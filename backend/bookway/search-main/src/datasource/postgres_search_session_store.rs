use super::*;

pub(crate) struct PostgresSearchSessionStore {
    pool: sqlx::PgPool,
}

impl PostgresSearchSessionStore {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchSessionStore for PostgresSearchSessionStore {
    async fn create(&self, session: SearchPipelineSession) -> Result<String, SearchSessionError> {
        let id = Uuid::now_v7().to_string();
        let state = serde_json::to_value(session)
            .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        sqlx::query(
            "WITH expired AS (DELETE FROM search_main_sessions WHERE expires_at <= now()) INSERT INTO search_main_sessions (session_id,state,expires_at) VALUES ($1,$2,now() + ($3 * interval '1 second'))",
        )
        .bind(&id)
        .bind(state)
        .bind(SEARCH_MAIN_SESSION_TTL.as_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        Ok(id)
    }

    async fn load(&self, id: &str) -> Result<Option<SearchPipelineSession>, SearchSessionError> {
        let state = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT state FROM search_main_sessions WHERE session_id = $1 AND expires_at > now()",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        state
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| SearchSessionError::Storage(error.to_string()))
    }

    async fn save(
        &self,
        id: &str,
        session: SearchPipelineSession,
    ) -> Result<bool, SearchSessionError> {
        let state = serde_json::to_value(session)
            .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        let result = sqlx::query(
            "UPDATE search_main_sessions SET state = $2, expires_at = now() + ($3 * interval '1 second') WHERE session_id = $1 AND expires_at > now()",
        )
        .bind(id)
        .bind(state)
        .bind(SEARCH_MAIN_SESSION_TTL.as_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, id: &str) -> Result<(), SearchSessionError> {
        sqlx::query("DELETE FROM search_main_sessions WHERE session_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        Ok(())
    }
}
