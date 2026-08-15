use std::sync::Arc;
use thiserror::Error;

use super::{
    api::{
        AuditDecisionDto, ContentAppealDto, ContentAppealPageDto, ContentAppealQueryRequest,
        ContentAuditRequest, ContentAuditResponse, ContentReportActionDto, ContentReportDto,
        ContentReportPageDto, ContentReportQueryRequest, CreateContentAppealRequest,
        CreateContentReportRequest, ReviewContentAppealRequest, ReviewContentReportRequest,
    },
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

    pub(crate) async fn report(
        &self,
        reporter_id: &str,
        content_id: &str,
        request: CreateContentReportRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentReportDto, AuditError> {
        if reporter_id.trim().is_empty() || content_id.trim().is_empty() {
            return Err(AuditError::Validation(
                "reporter and content must not be empty".to_string(),
            ));
        }
        if request.details.chars().count() > 1_000 {
            return Err(AuditError::Validation(
                "report details exceed 1000 characters".to_string(),
            ));
        }
        let idempotency_key = idempotency_key.map(|key| key.trim().to_string());
        if idempotency_key
            .as_deref()
            .is_some_and(|key| key.is_empty() || key.chars().count() > 128)
        {
            return Err(AuditError::Validation(
                "invalid idempotency key".to_string(),
            ));
        }
        let timestamp = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let report = ContentReportDto {
            id: uuid::Uuid::now_v7().to_string(),
            reporter_id: reporter_id.to_string(),
            content_id: content_id.to_string(),
            reason: request.reason,
            details: request.details.trim().to_string(),
            status: bookway_api::ContentReportStatusDto::Pending,
            created_at: timestamp.clone(),
            assignee_id: None,
            resolution: None,
            action: ContentReportActionDto::NoAction,
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
        query: ContentReportQueryRequest,
    ) -> Result<ContentReportPageDto, AuditError> {
        let limit = query.limit.unwrap_or(DEFAULT_REPORT_PAGE_SIZE);
        if !(1..=MAX_REPORT_PAGE_SIZE).contains(&limit) {
            return Err(AuditError::Validation(format!(
                "report limit must be between 1 and {MAX_REPORT_PAGE_SIZE}"
            )));
        }
        let cursor = match query.cursor.as_deref() {
            None => None,
            Some(value) if value.chars().count() <= 128 => ReportCursor::decode(value)
                .ok_or_else(|| AuditError::Validation("invalid report cursor".to_string()))
                .map(Some)?,
            Some(_) => return Err(AuditError::Validation("invalid report cursor".to_string())),
        };
        let mut items = self
            .repository
            .list_reports(&query, cursor.as_ref(), limit.saturating_add(1))
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(ReportCursor::from_report))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(ContentReportPageDto { items, next_cursor })
    }

    pub(crate) async fn review_report(
        &self,
        reviewer_id: &str,
        report_id: &str,
        request: ReviewContentReportRequest,
    ) -> Result<ContentReportDto, AuditError> {
        let reviewer_id = reviewer_id.trim();
        if reviewer_id.is_empty() || reviewer_id.chars().count() > MAX_REVIEWER_ID_LENGTH {
            return Err(AuditError::Validation(
                "invalid reviewer identity".to_string(),
            ));
        }
        if report_id.trim().is_empty() {
            return Err(AuditError::Validation(
                "report id must not be empty".to_string(),
            ));
        }
        let resolution = request.resolution.trim().to_string();
        if resolution.chars().count() > MAX_RESOLUTION_LENGTH {
            return Err(AuditError::Validation(
                "report resolution exceeds 1000 characters".to_string(),
            ));
        }
        match request.status {
            bookway_api::ContentReportStatusDto::Pending => {
                return Err(AuditError::Validation(
                    "pending is not a human review decision".to_string(),
                ));
            }
            bookway_api::ContentReportStatusDto::Reviewing if !resolution.is_empty() => {
                return Err(AuditError::Validation(
                    "reviewing reports cannot have a resolution".to_string(),
                ));
            }
            bookway_api::ContentReportStatusDto::Reviewing
                if request.action != ContentReportActionDto::NoAction =>
            {
                return Err(AuditError::Validation(
                    "reviewing reports cannot change content state".to_string(),
                ));
            }
            bookway_api::ContentReportStatusDto::Resolved
            | bookway_api::ContentReportStatusDto::Rejected
                if resolution.is_empty() =>
            {
                return Err(AuditError::Validation(
                    "terminal report decisions require a resolution".to_string(),
                ));
            }
            bookway_api::ContentReportStatusDto::Rejected
                if request.action != ContentReportActionDto::NoAction =>
            {
                return Err(AuditError::Validation(
                    "rejected reports cannot change content state".to_string(),
                ));
            }
            bookway_api::ContentReportStatusDto::Resolved
                if request.action == ContentReportActionDto::RestoreContent =>
            {
                return Err(AuditError::Validation(
                    "reports cannot restore content".to_string(),
                ));
            }
            _ => {}
        }
        self.repository
            .review_report(
                report_id.trim(),
                reviewer_id,
                ReviewContentReportRequest {
                    status: request.status,
                    resolution,
                    action: request.action,
                },
            )
            .await
            .map_err(AuditError::from)
    }

    pub(crate) async fn appeal(
        &self,
        appellant_id: &str,
        content_id: &str,
        request: CreateContentAppealRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentAppealDto, AuditError> {
        if appellant_id.trim().is_empty() || content_id.trim().is_empty() {
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
        let idempotency_key = idempotency_key.map(|key| key.trim().to_string());
        if idempotency_key
            .as_deref()
            .is_some_and(|key| key.is_empty() || key.chars().count() > 128)
        {
            return Err(AuditError::Validation(
                "invalid idempotency key".to_string(),
            ));
        }
        let timestamp = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let appeal = ContentAppealDto {
            id: uuid::Uuid::now_v7().to_string(),
            content_id: content_id.trim().to_string(),
            appellant_id: appellant_id.trim().to_string(),
            details,
            status: bookway_api::ContentAppealStatusDto::Pending,
            assignee_id: None,
            resolution: None,
            action: ContentReportActionDto::NoAction,
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
        query: ContentAppealQueryRequest,
    ) -> Result<ContentAppealPageDto, AuditError> {
        validate_appeal_filter(query.appellant_id.as_deref(), "appellant")?;
        validate_appeal_filter(query.content_id.as_deref(), "content")?;
        let limit = query.limit.unwrap_or(DEFAULT_REPORT_PAGE_SIZE);
        if !(1..=MAX_REPORT_PAGE_SIZE).contains(&limit) {
            return Err(AuditError::Validation(format!(
                "appeal limit must be between 1 and {MAX_REPORT_PAGE_SIZE}"
            )));
        }
        let cursor = match query.cursor.as_deref() {
            None => None,
            Some(value) if value.chars().count() <= 128 => ReportCursor::decode(value)
                .ok_or_else(|| AuditError::Validation("invalid appeal cursor".to_string()))
                .map(Some)?,
            Some(_) => return Err(AuditError::Validation("invalid appeal cursor".to_string())),
        };
        let mut items = self
            .repository
            .list_appeals(&query, cursor.as_ref(), limit.saturating_add(1))
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(ReportCursor::from_appeal))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(ContentAppealPageDto { items, next_cursor })
    }

    pub(crate) async fn review_appeal(
        &self,
        reviewer_id: &str,
        appeal_id: &str,
        request: ReviewContentAppealRequest,
    ) -> Result<ContentAppealDto, AuditError> {
        let reviewer_id = reviewer_id.trim();
        if reviewer_id.is_empty() || reviewer_id.chars().count() > MAX_REVIEWER_ID_LENGTH {
            return Err(AuditError::Validation(
                "invalid reviewer identity".to_string(),
            ));
        }
        if appeal_id.trim().is_empty() {
            return Err(AuditError::Validation(
                "appeal id must not be empty".to_string(),
            ));
        }
        let resolution = request.resolution.trim().to_string();
        if resolution.chars().count() > MAX_RESOLUTION_LENGTH {
            return Err(AuditError::Validation(
                "appeal resolution exceeds 1000 characters".to_string(),
            ));
        }
        match request.status {
            bookway_api::ContentAppealStatusDto::Pending => {
                return Err(AuditError::Validation(
                    "pending is not a human appeal decision".to_string(),
                ));
            }
            bookway_api::ContentAppealStatusDto::Reviewing
                if !resolution.is_empty() || request.action != ContentReportActionDto::NoAction =>
            {
                return Err(AuditError::Validation(
                    "reviewing appeals cannot have a resolution or content action".to_string(),
                ));
            }
            bookway_api::ContentAppealStatusDto::Resolved
            | bookway_api::ContentAppealStatusDto::Rejected
                if resolution.is_empty() =>
            {
                return Err(AuditError::Validation(
                    "terminal appeal decisions require a resolution".to_string(),
                ));
            }
            bookway_api::ContentAppealStatusDto::Rejected
                if request.action != ContentReportActionDto::NoAction =>
            {
                return Err(AuditError::Validation(
                    "rejected appeals cannot change content state".to_string(),
                ));
            }
            bookway_api::ContentAppealStatusDto::Resolved
                if request.action == ContentReportActionDto::RestrictContent =>
            {
                return Err(AuditError::Validation(
                    "appeals cannot restrict content".to_string(),
                ));
            }
            _ => {}
        }
        self.repository
            .review_appeal(
                appeal_id.trim(),
                reviewer_id,
                ReviewContentAppealRequest {
                    status: request.status,
                    resolution,
                    action: request.action,
                },
            )
            .await
            .map_err(AuditError::from)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{conf::Config, datasource::RepositoryError};
    use bookway_api::{ContentAppealStatusDto, ContentReportStatusDto, ReportReasonDto};

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

    #[tokio::test]
    async fn reports_are_idempotent_for_the_same_user_key() {
        let request = CreateContentReportRequest {
            reason: ReportReasonDto::Spam,
            details: "重复广告".to_string(),
        };
        let service = domain();
        let first = service
            .report(
                "user-a",
                "post-a",
                request.clone(),
                Some("report-1".to_string()),
            )
            .await
            .expect("first report");
        let second = service
            .report("user-a", "post-a", request, Some("report-1".to_string()))
            .await
            .expect("duplicate report");

        assert_eq!(first.id, second.id);
    }

    #[tokio::test]
    async fn rejects_reusing_an_idempotency_key_for_different_content() {
        let service = domain();
        let request = CreateContentReportRequest {
            reason: ReportReasonDto::Spam,
            details: String::new(),
        };
        service
            .report(
                "user-a",
                "post-a",
                request.clone(),
                Some("report-1".to_string()),
            )
            .await
            .expect("first report");

        let error = service
            .report("user-a", "post-b", request, Some("report-1".to_string()))
            .await
            .expect_err("idempotency key reuse must be rejected");

        assert!(matches!(error, AuditError::Validation(_)));
    }

    #[tokio::test]
    async fn lists_reports_with_a_stable_cursor_without_duplicates() {
        let service = domain();
        for content_id in ["post-a", "post-b", "post-c"] {
            service
                .report(
                    "user-a",
                    content_id,
                    CreateContentReportRequest {
                        reason: ReportReasonDto::Spam,
                        details: content_id.to_string(),
                    },
                    None,
                )
                .await
                .expect("create report");
        }

        let first = service
            .list_reports(ContentReportQueryRequest {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .expect("first page");
        let second = service
            .list_reports(ContentReportQueryRequest {
                limit: Some(1),
                cursor: first.next_cursor.clone(),
                ..Default::default()
            })
            .await
            .expect("second page");
        let third = service
            .list_reports(ContentReportQueryRequest {
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
            .report(
                "user-a",
                "post-a",
                CreateContentReportRequest {
                    reason: ReportReasonDto::Unsafe,
                    details: "危险内容".to_string(),
                },
                None,
            )
            .await
            .expect("create report");
        let resolution = "确认违反社区规则".to_string();
        let first = service
            .review_report(
                "moderator-a",
                &report.id,
                ReviewContentReportRequest {
                    status: ContentReportStatusDto::Resolved,
                    resolution: resolution.clone(),
                    action: ContentReportActionDto::RestrictContent,
                },
            )
            .await
            .expect("resolve report");
        let retry = service
            .review_report(
                "moderator-a",
                &report.id,
                ReviewContentReportRequest {
                    status: ContentReportStatusDto::Resolved,
                    resolution,
                    action: ContentReportActionDto::RestrictContent,
                },
            )
            .await
            .expect("retry resolution");
        let conflict = service
            .review_report(
                "moderator-a",
                &report.id,
                ReviewContentReportRequest {
                    status: ContentReportStatusDto::Rejected,
                    resolution: "不予处理".to_string(),
                    action: ContentReportActionDto::NoAction,
                },
            )
            .await
            .expect_err("terminal decision must not change");

        assert_eq!(first.status, ContentReportStatusDto::Resolved);
        assert_eq!(first.action, ContentReportActionDto::RestrictContent);
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
            .report(
                "user-a",
                "post-a",
                CreateContentReportRequest {
                    reason: ReportReasonDto::Spam,
                    details: String::new(),
                },
                None,
            )
            .await
            .expect("create report");

        let error = service
            .review_report(
                "moderator-a",
                &report.id,
                ReviewContentReportRequest {
                    status: ContentReportStatusDto::Rejected,
                    resolution: "不符合处置条件".to_string(),
                    action: ContentReportActionDto::RestrictContent,
                },
            )
            .await
            .expect_err("rejected report must not restrict content");

        assert!(matches!(error, AuditError::Validation(_)));
    }

    #[tokio::test]
    async fn appeals_are_idempotent_and_a_granted_restore_is_terminal() {
        let service = domain();
        let request = CreateContentAppealRequest {
            details: "这篇内容的上下文被误判，申请复核。".to_string(),
        };
        let first = service
            .appeal(
                "author-a",
                "post-a",
                request.clone(),
                Some("appeal-1".to_string()),
            )
            .await
            .expect("create appeal");
        let retry = service
            .appeal("author-a", "post-a", request, Some("appeal-1".to_string()))
            .await
            .expect("retry appeal");
        let resolved = service
            .review_appeal(
                "moderator-a",
                &first.id,
                ReviewContentAppealRequest {
                    status: ContentAppealStatusDto::Resolved,
                    resolution: "复核后恢复公开。".to_string(),
                    action: ContentReportActionDto::RestoreContent,
                },
            )
            .await
            .expect("grant appeal");
        let conflict = service
            .review_appeal(
                "moderator-a",
                &first.id,
                ReviewContentAppealRequest {
                    status: ContentAppealStatusDto::Rejected,
                    resolution: "不予恢复。".to_string(),
                    action: ContentReportActionDto::NoAction,
                },
            )
            .await
            .expect_err("terminal appeal cannot change");

        assert_eq!(first.id, retry.id);
        assert_eq!(resolved.action, ContentReportActionDto::RestoreContent);
        assert!(matches!(
            conflict,
            AuditError::Repository(RepositoryError::AppealConflict)
        ));
    }

    #[tokio::test]
    async fn appeal_history_is_scoped_to_the_appellant_across_all_statuses() {
        let service = domain();
        let pending = service
            .appeal(
                "author-a",
                "post-a",
                CreateContentAppealRequest {
                    details: "请复核这篇内容。".to_string(),
                },
                None,
            )
            .await
            .expect("create pending appeal");
        let rejected = service
            .appeal(
                "author-a",
                "post-b",
                CreateContentAppealRequest {
                    details: "补充上下文后再次申请复核。".to_string(),
                },
                None,
            )
            .await
            .expect("create rejected appeal");
        service
            .appeal(
                "author-b",
                "post-c",
                CreateContentAppealRequest {
                    details: "这不是 author-a 的申诉。".to_string(),
                },
                None,
            )
            .await
            .expect("create other author's appeal");
        service
            .review_appeal(
                "moderator-a",
                &rejected.id,
                ReviewContentAppealRequest {
                    status: ContentAppealStatusDto::Rejected,
                    resolution: "当前材料不足以推翻原处置。".to_string(),
                    action: ContentReportActionDto::NoAction,
                },
            )
            .await
            .expect("reject appeal");

        let page = service
            .list_appeals(ContentAppealQueryRequest {
                appellant_id: Some("author-a".to_string()),
                ..Default::default()
            })
            .await
            .expect("list author appeal history");

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
                .any(|appeal| appeal.status == ContentAppealStatusDto::Rejected)
        );
    }
}
