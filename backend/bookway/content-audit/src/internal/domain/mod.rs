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
pub(crate) struct AuditService {
    repository: Arc<AuditRepository>,
    blocked: Arc<Vec<String>>,
    reviewing: Arc<Vec<String>>,
}
impl AuditService {
    pub(crate) fn new(
        repository: Arc<AuditRepository>,
        blocked: Vec<String>,
        reviewing: Vec<String>,
    ) -> Self {
        Self {
            repository,
            blocked: Arc::new(blocked),
            reviewing: Arc::new(reviewing),
        }
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
