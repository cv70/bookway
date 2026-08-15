use std::sync::Arc;

use thiserror::Error;

use crate::{
    api::pb,
    datasource::{AuditRepository, ReportCursor, RepositoryError},
};

const DEFAULT_REPORT_PAGE_SIZE: usize = 50;
const MAX_REPORT_PAGE_SIZE: usize = 100;
const MAX_REVIEWER_ID_LENGTH: usize = 256;
const MAX_RESOLUTION_LENGTH: usize = 1_000;

#[derive(Debug, Error)]
pub(crate) enum AuditError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: crate::conf::Config,
    repository: Arc<AuditRepository>,
    blocked: Arc<Vec<String>>,
    reviewing: Arc<Vec<String>>,
}

impl Domain {
    pub async fn new(config: crate::conf::Config) -> Result<Self, bookway_data::DataError> {
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
        request: pb::AuditRequest,
    ) -> Result<pb::AuditResponse, AuditError> {
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
            (pb::AuditDecision::Restricted, 1.0, blocked)
        } else if !reviewing.is_empty() {
            (pb::AuditDecision::Reviewing, 0.65, reviewing)
        } else {
            (pb::AuditDecision::Approved, 0.05, Vec::new())
        };
        let response = pb::AuditResponse {
            decision: decision as i32,
            risk_score,
            reasons,
            provider: "bookway-rules-v1".to_string(),
        };
        self.repository.store(&request, &response).await?;
        Ok(response)
    }

    pub(crate) async fn report(
        &self,
        request: pb::CreateReportRequest,
    ) -> Result<pb::ContentReport, AuditError> {
        if request.reporter_id.trim().is_empty() || request.content_id.trim().is_empty() {
            return Err(AuditError::Validation(
                "reporter and content must not be empty".to_string(),
            ));
        }
        let reason = report_reason(request.reason)?;
        if request.details.chars().count() > 1_000 {
            return Err(AuditError::Validation(
                "report details exceed 1000 characters".to_string(),
            ));
        }
        let idempotency_key = normalize_idempotency_key(request.idempotency_key)?;
        let timestamp = timestamp();
        let report = pb::ContentReport {
            id: uuid::Uuid::now_v7().to_string(),
            reporter_id: request.reporter_id.trim().to_string(),
            content_id: request.content_id.trim().to_string(),
            reason: reason as i32,
            details: request.details.trim().to_string(),
            status: pb::ReportStatus::Pending as i32,
            created_at: timestamp.clone(),
            assignee_id: None,
            resolution: None,
            action: pb::ContentAction::NoAction as i32,
            updated_at: timestamp,
        };
        let stored = self
            .repository
            .store_report(report.clone(), idempotency_key)
            .await?;
        if stored.reporter_id != report.reporter_id
            || stored.content_id != report.content_id
            || stored.reason != report.reason
            || stored.details != report.details
        {
            return Err(AuditError::Validation(
                "idempotency key was already used for another report".to_string(),
            ));
        }
        Ok(stored)
    }

    pub(crate) async fn list_reports(
        &self,
        request: pb::ListReportsRequest,
    ) -> Result<pb::ReportPage, AuditError> {
        if let Some(status) = request.status {
            report_status(status)?;
        }
        let limit = request
            .limit
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_REPORT_PAGE_SIZE);
        if !(1..=MAX_REPORT_PAGE_SIZE).contains(&limit) {
            return Err(AuditError::Validation(format!(
                "report limit must be between 1 and {MAX_REPORT_PAGE_SIZE}"
            )));
        }
        let cursor = decode_cursor(request.cursor.as_deref(), "report")?;
        let mut items = self
            .repository
            .list_reports(&request, cursor.as_ref(), limit.saturating_add(1))
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(ReportCursor::from_report))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::ReportPage { items, next_cursor })
    }

    pub(crate) async fn review_report(
        &self,
        mut request: pb::ReviewReportRequest,
    ) -> Result<pb::ContentReport, AuditError> {
        let reviewer_id = request.reviewer_id.trim().to_string();
        if reviewer_id.is_empty() || reviewer_id.chars().count() > MAX_REVIEWER_ID_LENGTH {
            return Err(AuditError::Validation(
                "invalid reviewer identity".to_string(),
            ));
        }
        let report_id = request.report_id.trim().to_string();
        if report_id.is_empty() {
            return Err(AuditError::Validation(
                "report id must not be empty".to_string(),
            ));
        }
        let status = report_status(request.status)?;
        let action = content_action(request.action)?;
        request.resolution = request.resolution.trim().to_string();
        if request.resolution.chars().count() > MAX_RESOLUTION_LENGTH {
            return Err(AuditError::Validation(
                "report resolution exceeds 1000 characters".to_string(),
            ));
        }
        match status {
            pb::ReportStatus::Pending => {
                return Err(AuditError::Validation(
                    "pending is not a human review decision".to_string(),
                ));
            }
            pb::ReportStatus::Reviewing if !request.resolution.is_empty() => {
                return Err(AuditError::Validation(
                    "reviewing reports cannot have a resolution".to_string(),
                ));
            }
            pb::ReportStatus::Reviewing if action != pb::ContentAction::NoAction => {
                return Err(AuditError::Validation(
                    "reviewing reports cannot change content state".to_string(),
                ));
            }
            pb::ReportStatus::Resolved | pb::ReportStatus::Rejected
                if request.resolution.is_empty() =>
            {
                return Err(AuditError::Validation(
                    "terminal report decisions require a resolution".to_string(),
                ));
            }
            pb::ReportStatus::Rejected if action != pb::ContentAction::NoAction => {
                return Err(AuditError::Validation(
                    "rejected reports cannot change content state".to_string(),
                ));
            }
            pb::ReportStatus::Resolved if action == pb::ContentAction::Restore => {
                return Err(AuditError::Validation(
                    "reports cannot restore content".to_string(),
                ));
            }
            _ => {}
        }
        self.repository
            .review_report(&report_id, &reviewer_id, request)
            .await
            .map_err(AuditError::from)
    }

    pub(crate) async fn appeal(
        &self,
        request: pb::CreateAppealRequest,
    ) -> Result<pb::ContentAppeal, AuditError> {
        if request.appellant_id.trim().is_empty() || request.content_id.trim().is_empty() {
            return Err(AuditError::Validation(
                "appellant and content must not be empty".to_string(),
            ));
        }
        let details = request.details.trim().to_string();
        if details.is_empty() || details.chars().count() > MAX_RESOLUTION_LENGTH {
            return Err(AuditError::Validation(
                "appeal details must be between 1 and 1000 characters".to_string(),
            ));
        }
        let idempotency_key = normalize_idempotency_key(request.idempotency_key)?;
        let timestamp = timestamp();
        let appeal = pb::ContentAppeal {
            id: uuid::Uuid::now_v7().to_string(),
            content_id: request.content_id.trim().to_string(),
            appellant_id: request.appellant_id.trim().to_string(),
            details,
            status: pb::AppealStatus::Pending as i32,
            assignee_id: None,
            resolution: None,
            action: pb::ContentAction::NoAction as i32,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };
        let stored = self
            .repository
            .store_appeal(appeal.clone(), idempotency_key)
            .await?;
        if stored.appellant_id != appeal.appellant_id
            || stored.content_id != appeal.content_id
            || stored.details != appeal.details
        {
            return Err(AuditError::Validation(
                "idempotency key was already used for another appeal".to_string(),
            ));
        }
        Ok(stored)
    }

    pub(crate) async fn list_appeals(
        &self,
        request: pb::ListAppealsRequest,
    ) -> Result<pb::AppealPage, AuditError> {
        if let Some(status) = request.status {
            appeal_status(status)?;
        }
        validate_appeal_filter(request.appellant_id.as_deref(), "appellant")?;
        validate_appeal_filter(request.content_id.as_deref(), "content")?;
        let limit = request
            .limit
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_REPORT_PAGE_SIZE);
        if !(1..=MAX_REPORT_PAGE_SIZE).contains(&limit) {
            return Err(AuditError::Validation(format!(
                "appeal limit must be between 1 and {MAX_REPORT_PAGE_SIZE}"
            )));
        }
        let cursor = decode_cursor(request.cursor.as_deref(), "appeal")?;
        let mut items = self
            .repository
            .list_appeals(&request, cursor.as_ref(), limit.saturating_add(1))
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(ReportCursor::from_appeal))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::AppealPage { items, next_cursor })
    }

    pub(crate) async fn review_appeal(
        &self,
        mut request: pb::ReviewAppealRequest,
    ) -> Result<pb::ContentAppeal, AuditError> {
        let reviewer_id = request.reviewer_id.trim().to_string();
        if reviewer_id.is_empty() || reviewer_id.chars().count() > MAX_REVIEWER_ID_LENGTH {
            return Err(AuditError::Validation(
                "invalid reviewer identity".to_string(),
            ));
        }
        let appeal_id = request.appeal_id.trim().to_string();
        if appeal_id.is_empty() {
            return Err(AuditError::Validation(
                "appeal id must not be empty".to_string(),
            ));
        }
        let status = appeal_status(request.status)?;
        let action = content_action(request.action)?;
        request.resolution = request.resolution.trim().to_string();
        if request.resolution.chars().count() > MAX_RESOLUTION_LENGTH {
            return Err(AuditError::Validation(
                "appeal resolution exceeds 1000 characters".to_string(),
            ));
        }
        match status {
            pb::AppealStatus::Pending => {
                return Err(AuditError::Validation(
                    "pending is not a human appeal decision".to_string(),
                ));
            }
            pb::AppealStatus::Reviewing
                if !request.resolution.is_empty() || action != pb::ContentAction::NoAction =>
            {
                return Err(AuditError::Validation(
                    "reviewing appeals cannot have a resolution or content action".to_string(),
                ));
            }
            pb::AppealStatus::Resolved | pb::AppealStatus::Rejected
                if request.resolution.is_empty() =>
            {
                return Err(AuditError::Validation(
                    "terminal appeal decisions require a resolution".to_string(),
                ));
            }
            pb::AppealStatus::Rejected if action != pb::ContentAction::NoAction => {
                return Err(AuditError::Validation(
                    "rejected appeals cannot change content state".to_string(),
                ));
            }
            pb::AppealStatus::Resolved if action == pb::ContentAction::Restrict => {
                return Err(AuditError::Validation(
                    "appeals cannot restrict content".to_string(),
                ));
            }
            _ => {}
        }
        self.repository
            .review_appeal(&appeal_id, &reviewer_id, request)
            .await
            .map_err(AuditError::from)
    }
}

fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn normalize_idempotency_key(value: Option<String>) -> Result<Option<String>, AuditError> {
    let value = value.map(|key| key.trim().to_string());
    if value
        .as_deref()
        .is_some_and(|key| key.is_empty() || key.chars().count() > 128)
    {
        return Err(AuditError::Validation(
            "invalid idempotency key".to_string(),
        ));
    }
    Ok(value)
}

fn decode_cursor(value: Option<&str>, label: &str) -> Result<Option<ReportCursor>, AuditError> {
    match value {
        None => Ok(None),
        Some(value) if value.chars().count() <= 128 => ReportCursor::decode(value)
            .ok_or_else(|| AuditError::Validation(format!("invalid {label} cursor")))
            .map(Some),
        Some(_) => Err(AuditError::Validation(format!("invalid {label} cursor"))),
    }
}

fn validate_appeal_filter(value: Option<&str>, label: &str) -> Result<(), AuditError> {
    if value.is_some_and(|value| value.trim().is_empty() || value.chars().count() > 128) {
        return Err(AuditError::Validation(format!(
            "invalid appeal {label} filter"
        )));
    }
    Ok(())
}

fn report_reason(value: i32) -> Result<pb::ReportReason, AuditError> {
    pb::ReportReason::try_from(value)
        .map_err(|_| AuditError::Validation("invalid report reason".to_string()))
}

fn report_status(value: i32) -> Result<pb::ReportStatus, AuditError> {
    pb::ReportStatus::try_from(value)
        .map_err(|_| AuditError::Validation("invalid report status".to_string()))
}

fn appeal_status(value: i32) -> Result<pb::AppealStatus, AuditError> {
    pb::AppealStatus::try_from(value)
        .map_err(|_| AuditError::Validation("invalid appeal status".to_string()))
}

fn content_action(value: i32) -> Result<pb::ContentAction, AuditError> {
    pb::ContentAction::try_from(value)
        .map_err(|_| AuditError::Validation("invalid content action".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{conf::Config, datasource::RepositoryError};

    fn domain() -> Domain {
        Domain {
            config: Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid address"),
                blocked: Vec::new(),
                reviewing: Vec::new(),
            },
            repository: Arc::new(AuditRepository::new(None)),
            blocked: Arc::new(Vec::new()),
            reviewing: Arc::new(Vec::new()),
        }
    }

    fn report_request(content_id: &str, key: Option<&str>) -> pb::CreateReportRequest {
        pb::CreateReportRequest {
            reporter_id: "user-a".to_string(),
            content_id: content_id.to_string(),
            idempotency_key: key.map(str::to_string),
            reason: pb::ReportReason::Spam as i32,
            details: "重复广告".to_string(),
        }
    }

    #[tokio::test]
    async fn reports_are_idempotent_for_the_same_user_key() {
        let service = domain();
        let first = service
            .report(report_request("post-a", Some("report-1")))
            .await
            .expect("first report");
        let second = service
            .report(report_request("post-a", Some("report-1")))
            .await
            .expect("duplicate report");
        assert_eq!(first.id, second.id);
    }

    #[tokio::test]
    async fn rejects_reusing_an_idempotency_key_for_different_content() {
        let service = domain();
        service
            .report(report_request("post-a", Some("report-1")))
            .await
            .expect("first report");
        let error = service
            .report(report_request("post-b", Some("report-1")))
            .await
            .expect_err("idempotency key reuse must be rejected");
        assert!(matches!(error, AuditError::Validation(_)));
    }

    #[tokio::test]
    async fn lists_reports_with_a_stable_cursor_without_duplicates() {
        let service = domain();
        for content_id in ["post-a", "post-b", "post-c"] {
            service
                .report(report_request(content_id, None))
                .await
                .expect("create report");
        }
        let first = service
            .list_reports(pb::ListReportsRequest {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .expect("first page");
        let second = service
            .list_reports(pb::ListReportsRequest {
                limit: Some(1),
                cursor: first.next_cursor.clone(),
                ..Default::default()
            })
            .await
            .expect("second page");
        let third = service
            .list_reports(pb::ListReportsRequest {
                limit: Some(1),
                cursor: second.next_cursor.clone(),
                ..Default::default()
            })
            .await
            .expect("third page");
        let ids = [
            first.items[0].id.as_str(),
            second.items[0].id.as_str(),
            third.items[0].id.as_str(),
        ];
        assert_eq!(
            ids.into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            3
        );
        assert!(third.next_cursor.is_none());
    }

    #[tokio::test]
    async fn terminal_report_decisions_are_idempotent_but_cannot_change() {
        let service = domain();
        let report = service
            .report(pb::CreateReportRequest {
                reason: pb::ReportReason::Unsafe as i32,
                ..report_request("post-a", None)
            })
            .await
            .expect("create report");
        let request = pb::ReviewReportRequest {
            reviewer_id: "moderator-a".to_string(),
            report_id: report.id.clone(),
            status: pb::ReportStatus::Resolved as i32,
            resolution: "确认违反社区规则".to_string(),
            action: pb::ContentAction::Restrict as i32,
        };
        let first = service
            .review_report(request.clone())
            .await
            .expect("resolve report");
        let retry = service
            .review_report(request)
            .await
            .expect("retry resolution");
        let conflict = service
            .review_report(pb::ReviewReportRequest {
                status: pb::ReportStatus::Rejected as i32,
                resolution: "不予处理".to_string(),
                action: pb::ContentAction::NoAction as i32,
                reviewer_id: "moderator-a".to_string(),
                report_id: report.id,
            })
            .await
            .expect_err("terminal decision must not change");
        assert_eq!(first.status, pb::ReportStatus::Resolved as i32);
        assert_eq!(first.action, pb::ContentAction::Restrict as i32);
        assert_eq!(first.updated_at, retry.updated_at);
        assert!(matches!(
            conflict,
            AuditError::Repository(RepositoryError::ReportConflict)
        ));
    }

    #[tokio::test]
    async fn rejects_content_actions_without_a_resolved_decision() {
        let service = domain();
        let report = service
            .report(report_request("post-a", None))
            .await
            .expect("create report");
        let error = service
            .review_report(pb::ReviewReportRequest {
                reviewer_id: "moderator-a".to_string(),
                report_id: report.id,
                status: pb::ReportStatus::Rejected as i32,
                resolution: "不符合处置条件".to_string(),
                action: pb::ContentAction::Restrict as i32,
            })
            .await
            .expect_err("rejected report must not restrict content");
        assert!(matches!(error, AuditError::Validation(_)));
    }

    #[tokio::test]
    async fn appeals_are_idempotent_and_a_granted_restore_is_terminal() {
        let service = domain();
        let request = pb::CreateAppealRequest {
            appellant_id: "author-a".to_string(),
            content_id: "post-a".to_string(),
            idempotency_key: Some("appeal-1".to_string()),
            details: "这篇内容的上下文被误判，申请复核。".to_string(),
        };
        let first = service
            .appeal(request.clone())
            .await
            .expect("create appeal");
        let retry = service.appeal(request).await.expect("retry appeal");
        let resolved = service
            .review_appeal(pb::ReviewAppealRequest {
                reviewer_id: "moderator-a".to_string(),
                appeal_id: first.id.clone(),
                status: pb::AppealStatus::Resolved as i32,
                resolution: "复核后恢复公开。".to_string(),
                action: pb::ContentAction::Restore as i32,
            })
            .await
            .expect("grant appeal");
        let conflict = service
            .review_appeal(pb::ReviewAppealRequest {
                reviewer_id: "moderator-a".to_string(),
                appeal_id: first.id.clone(),
                status: pb::AppealStatus::Rejected as i32,
                resolution: "不予恢复。".to_string(),
                action: pb::ContentAction::NoAction as i32,
            })
            .await
            .expect_err("terminal appeal cannot change");
        assert_eq!(first.id, retry.id);
        assert_eq!(resolved.action, pb::ContentAction::Restore as i32);
        assert!(matches!(
            conflict,
            AuditError::Repository(RepositoryError::AppealConflict)
        ));
    }

    #[tokio::test]
    async fn appeal_history_is_scoped_to_the_appellant_across_all_statuses() {
        let service = domain();
        let create = |appellant_id: &str, content_id: &str| pb::CreateAppealRequest {
            appellant_id: appellant_id.to_string(),
            content_id: content_id.to_string(),
            idempotency_key: None,
            details: "请复核这篇内容。".to_string(),
        };
        let pending = service
            .appeal(create("author-a", "post-a"))
            .await
            .expect("pending");
        let rejected = service
            .appeal(create("author-a", "post-b"))
            .await
            .expect("rejected");
        service
            .appeal(create("author-b", "post-c"))
            .await
            .expect("other author");
        service
            .review_appeal(pb::ReviewAppealRequest {
                reviewer_id: "moderator-a".to_string(),
                appeal_id: rejected.id.clone(),
                status: pb::AppealStatus::Rejected as i32,
                resolution: "当前材料不足以推翻原处置。".to_string(),
                action: pb::ContentAction::NoAction as i32,
            })
            .await
            .expect("reject appeal");
        let page = service
            .list_appeals(pb::ListAppealsRequest {
                appellant_id: Some("author-a".to_string()),
                ..Default::default()
            })
            .await
            .expect("list author history");
        assert_eq!(page.items.len(), 2);
        assert!(
            page.items
                .iter()
                .all(|appeal| appeal.appellant_id == "author-a")
        );
        assert!(page.items.iter().any(|appeal| appeal.id == pending.id));
        assert!(
            page.items
                .iter()
                .any(|appeal| appeal.status == pb::AppealStatus::Rejected as i32)
        );
    }
}
