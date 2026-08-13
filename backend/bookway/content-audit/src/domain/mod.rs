use std::sync::Arc;
use thiserror::Error;

use super::{
    api::{AuditDecisionDto, ContentAuditRequest, ContentAuditResponse},
    datasource::{AuditRepository, RepositoryError},
};

#[derive(Debug, Error)]
pub(crate) enum AuditError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: crate::conf::Config,
    repository: Arc<AuditRepository>,
    blocked: Arc<Vec<String>>,
    reviewing: Arc<Vec<String>>,
}
impl Domain {
    pub(crate) async fn new(config: crate::conf::Config) -> Result<Self, bookway_data::DataError> {
        let pool = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Postgres => Some(bookway_data::postgres_pool().await?),
            bookway_data::StorageMode::Memory => None,
        };
        let blocked = config.blocked.clone();
        let reviewing = config.reviewing.clone();
        Ok(Self {
            config,
            repository: Arc::new(AuditRepository::new(pool)),
            blocked: Arc::new(blocked),
            reviewing: Arc::new(reviewing),
        })
    }
    pub(crate) async fn audit(
        &self,
        request: ContentAuditRequest,
    ) -> Result<ContentAuditResponse, AuditError> {
        let text = format!("{} {}", request.title, request.body);
        let blocked = self
            .blocked
            .iter()
            .filter(|term| text.contains(term.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let reviewing = self
            .reviewing
            .iter()
            .filter(|term| text.contains(term.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let (decision, risk_score, reasons) = if !blocked.is_empty() {
            (AuditDecisionDto::Restricted, 1.0, blocked)
        } else if !reviewing.is_empty() {
            (AuditDecisionDto::Reviewing, 0.65, reviewing)
        } else {
            (AuditDecisionDto::Approved, 0.05, Vec::new())
        };
        let response = ContentAuditResponse {
            decision,
            risk_score,
            reasons,
            provider: "bookway-rules-v1".to_string(),
        };
        self.repository.store(&request, &response).await?;
        Ok(response)
    }
}
