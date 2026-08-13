use super::api::{AuditDecisionDto, ContentAuditRequest, ContentAuditResponse};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[derive(Clone)]
pub(crate) struct AuditRepository {
    pool: Option<sqlx::PgPool>,
}
impl AuditRepository {
    pub(crate) fn new(pool: Option<sqlx::PgPool>) -> Self {
        Self { pool }
    }
    pub(crate) async fn store(
        &self,
        request: &ContentAuditRequest,
        response: &ContentAuditResponse,
    ) -> Result<(), RepositoryError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let decision = match response.decision {
            AuditDecisionDto::Approved => "approved",
            AuditDecisionDto::Reviewing => "reviewing",
            AuditDecisionDto::Restricted => "restricted",
        };
        sqlx::query("INSERT INTO content_audits (content_id,version,decision,risk_score,reasons,provider) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (content_id,version) DO UPDATE SET decision=excluded.decision,risk_score=excluded.risk_score,reasons=excluded.reasons,provider=excluded.provider")
            .bind(&request.content_id).bind(i32::try_from(request.version).unwrap_or(i32::MAX)).bind(decision).bind(response.risk_score).bind(serde_json::json!(response.reasons)).bind(&response.provider).execute(pool).await.map_err(RepositoryError::Database)?;
        Ok(())
    }
}
