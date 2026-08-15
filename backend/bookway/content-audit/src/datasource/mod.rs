use std::{collections::HashMap, sync::Arc};

use super::api::{
    AuditDecisionDto, ContentAppealDto, ContentAppealQueryRequest, ContentAuditRequest,
    ContentAuditResponse, ContentReportActionDto, ContentReportDto, ContentReportQueryRequest,
    ReviewContentAppealRequest, ReviewContentReportRequest,
};
use bookway_api::{ContentAppealStatusDto, ContentReportStatusDto};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("report {0} was not found")]
    ReportNotFound(String),
    #[error("appeal {0} was not found")]
    AppealNotFound(String),
    #[error("report is already in a terminal state")]
    ReportConflict,
    #[error("appeal is already in a terminal state")]
    AppealConflict,
    #[error("invalid report review: {0}")]
    InvalidReview(String),
    #[error("invalid appeal review: {0}")]
    InvalidAppealReview(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored report is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
}

type ModerationRow = (
    serde_json::Value,
    String,
    Option<String>,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReportCursor {
    created_at: OffsetDateTime,
    id: String,
}

impl ReportCursor {
    pub(crate) fn decode(value: &str) -> Option<Self> {
        let (created_at, id) = value.split_once('|')?;
        let created_at = OffsetDateTime::parse(created_at, &Rfc3339).ok()?;
        (!id.is_empty()).then(|| Self {
            created_at,
            id: id.to_string(),
        })
    }

    pub(crate) fn from_report(report: &ContentReportDto) -> Option<Self> {
        Self::from_values(&report.created_at, &report.id)
    }

    pub(crate) fn from_appeal(appeal: &ContentAppealDto) -> Option<Self> {
        Self::from_values(&appeal.created_at, &appeal.id)
    }

    fn from_values(created_at: &str, id: &str) -> Option<Self> {
        Some(Self {
            created_at: OffsetDateTime::parse(created_at, &Rfc3339).ok()?,
            id: id.to_string(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.created_at), self.id)
    }
}

#[derive(Clone)]
pub(crate) struct AuditRepository {
    pool: Option<sqlx::PgPool>,
    reports: Arc<RwLock<MemoryReports>>,
    appeals: Arc<RwLock<MemoryAppeals>>,
}

#[derive(Default)]
struct MemoryReports {
    by_id: HashMap<String, ContentReportDto>,
    idempotency: HashMap<(String, String), String>,
}

#[derive(Default)]
struct MemoryAppeals {
    by_id: HashMap<String, ContentAppealDto>,
    idempotency: HashMap<(String, String), String>,
}
impl AuditRepository {
    pub(crate) fn new(pool: Option<sqlx::PgPool>) -> Self {
        Self {
            pool,
            reports: Arc::new(RwLock::new(MemoryReports::default())),
            appeals: Arc::new(RwLock::new(MemoryAppeals::default())),
        }
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

    pub(crate) async fn store_report(
        &self,
        report: ContentReportDto,
        idempotency_key: Option<String>,
    ) -> Result<ContentReportDto, RepositoryError> {
        let Some(pool) = &self.pool else {
            let Some(idempotency_key) = idempotency_key else {
                self.reports
                    .write()
                    .await
                    .by_id
                    .insert(report.id.clone(), report.clone());
                return Ok(report);
            };
            let mut reports = self.reports.write().await;
            let key = (report.reporter_id.clone(), idempotency_key);
            if let Some(existing) = reports.idempotency.get(&key)
                && let Some(existing) = reports.by_id.get(existing)
            {
                return Ok(existing.clone());
            }
            reports.idempotency.insert(key, report.id.clone());
            reports.by_id.insert(report.id.clone(), report.clone());
            return Ok(report);
        };
        let payload = serde_json::to_value(&report).map_err(RepositoryError::Serialization)?;
        let stored = sqlx::query_as::<_, ModerationRow>(
            "INSERT INTO community_reports (id,reporter_id,content_id,reason,details,status,idempotency_key,payload,created_at) VALUES ($1,$2,$3,$4,$5,'pending',$6,$7,$8::timestamptz) ON CONFLICT (reporter_id,idempotency_key) DO UPDATE SET reporter_id=EXCLUDED.reporter_id RETURNING payload,status,assignee_id,resolution,created_at,updated_at",
        )
        .bind(&report.id)
        .bind(&report.reporter_id)
        .bind(&report.content_id)
        .bind(report_reason(report.reason))
        .bind(&report.details)
        .bind(idempotency_key)
        .bind(payload)
        .bind(&report.created_at)
        .fetch_one(pool)
        .await
        .map_err(RepositoryError::Database)?;
        hydrate_report(stored)
    }

    pub(crate) async fn list_reports(
        &self,
        query: &ContentReportQueryRequest,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<ContentReportDto>, RepositoryError> {
        let status = query.status.unwrap_or_default();
        let Some(pool) = &self.pool else {
            let mut reports = self
                .reports
                .read()
                .await
                .by_id
                .values()
                .filter(|report| report.status == status)
                .cloned()
                .collect::<Vec<_>>();
            reports.sort_by_key(ReportCursor::from_report);
            if let Some(cursor) = cursor {
                reports.retain(|report| {
                    ReportCursor::from_report(report).is_some_and(|value| value > cursor.clone())
                });
            }
            reports.truncate(limit);
            return Ok(reports);
        };
        let rows = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM community_reports WHERE status = $1 AND ($2::TIMESTAMPTZ IS NULL OR (created_at,id) > ($2::TIMESTAMPTZ,$3)) ORDER BY created_at ASC,id ASC LIMIT $4",
        )
        .bind(report_status_name(status))
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(hydrate_report)
            .collect::<Result<Vec<_>, RepositoryError>>()
    }

    pub(crate) async fn review_report(
        &self,
        report_id: &str,
        reviewer_id: &str,
        request: ReviewContentReportRequest,
    ) -> Result<ContentReportDto, RepositoryError> {
        let Some(pool) = &self.pool else {
            let mut reports = self.reports.write().await;
            let report = reports
                .by_id
                .get_mut(report_id)
                .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))?;
            return apply_review(report, reviewer_id, &request);
        };
        let mut tx = pool.begin().await.map_err(RepositoryError::Database)?;
        let row = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM community_reports WHERE id = $1 FOR UPDATE",
        )
        .bind(report_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))?;
        let mut report = hydrate_report(row)?;
        if is_terminal(report.status) {
            let reviewed = apply_review(&mut report, reviewer_id, &request)?;
            ensure_report_restriction_job(&mut tx, &reviewed).await?;
            tx.commit().await.map_err(RepositoryError::Database)?;
            return Ok(reviewed);
        }
        let reviewed = apply_review(&mut report, reviewer_id, &request)?;
        let payload = serde_json::to_value(&reviewed).map_err(RepositoryError::Serialization)?;
        let updated_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "UPDATE community_reports SET status = $2,assignee_id = $3,resolution = $4,payload = $5,updated_at = now() WHERE id = $1 RETURNING updated_at",
        )
        .bind(report_id)
        .bind(report_status_name(reviewed.status))
        .bind(&reviewed.assignee_id)
        .bind(&reviewed.resolution)
        .bind(payload)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?;
        ensure_report_restriction_job(&mut tx, &reviewed).await?;
        tx.commit().await.map_err(RepositoryError::Database)?;
        report.updated_at = format_timestamp(updated_at);
        Ok(report)
    }

    pub(crate) async fn store_appeal(
        &self,
        appeal: ContentAppealDto,
        idempotency_key: Option<String>,
    ) -> Result<ContentAppealDto, RepositoryError> {
        let Some(pool) = &self.pool else {
            let Some(idempotency_key) = idempotency_key else {
                self.appeals
                    .write()
                    .await
                    .by_id
                    .insert(appeal.id.clone(), appeal.clone());
                return Ok(appeal);
            };
            let mut appeals = self.appeals.write().await;
            let key = (appeal.appellant_id.clone(), idempotency_key);
            if let Some(existing) = appeals.idempotency.get(&key)
                && let Some(existing) = appeals.by_id.get(existing)
            {
                return Ok(existing.clone());
            }
            appeals.idempotency.insert(key, appeal.id.clone());
            appeals.by_id.insert(appeal.id.clone(), appeal.clone());
            return Ok(appeal);
        };
        let payload = serde_json::to_value(&appeal).map_err(RepositoryError::Serialization)?;
        let stored = sqlx::query_as::<_, ModerationRow>(
            "INSERT INTO content_appeals (id,appellant_id,content_id,details,status,idempotency_key,payload,created_at) VALUES ($1,$2,$3,$4,'pending',$5,$6,$7::timestamptz) ON CONFLICT (appellant_id,idempotency_key) DO UPDATE SET appellant_id=EXCLUDED.appellant_id RETURNING payload,status,assignee_id,resolution,created_at,updated_at",
        )
        .bind(&appeal.id)
        .bind(&appeal.appellant_id)
        .bind(&appeal.content_id)
        .bind(&appeal.details)
        .bind(idempotency_key)
        .bind(payload)
        .bind(&appeal.created_at)
        .fetch_one(pool)
        .await
        .map_err(RepositoryError::Database)?;
        hydrate_appeal(stored)
    }

    pub(crate) async fn list_appeals(
        &self,
        query: &ContentAppealQueryRequest,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<ContentAppealDto>, RepositoryError> {
        let Some(pool) = &self.pool else {
            let mut appeals = self
                .appeals
                .read()
                .await
                .by_id
                .values()
                .filter(|appeal| query.status.is_none_or(|status| appeal.status == status))
                .filter(|appeal| {
                    query
                        .appellant_id
                        .as_deref()
                        .is_none_or(|appellant_id| appeal.appellant_id == appellant_id)
                })
                .filter(|appeal| {
                    query
                        .content_id
                        .as_deref()
                        .is_none_or(|content_id| appeal.content_id == content_id)
                })
                .cloned()
                .collect::<Vec<_>>();
            appeals.sort_by_key(ReportCursor::from_appeal);
            if let Some(cursor) = cursor {
                appeals.retain(|appeal| {
                    ReportCursor::from_appeal(appeal).is_some_and(|value| value > cursor.clone())
                });
            }
            appeals.truncate(limit);
            return Ok(appeals);
        };
        let rows = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM content_appeals WHERE ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR appellant_id = $2) AND ($3::text IS NULL OR content_id = $3) AND ($4::TIMESTAMPTZ IS NULL OR (created_at,id) > ($4::TIMESTAMPTZ,$5)) ORDER BY created_at ASC,id ASC LIMIT $6",
        )
        .bind(query.status.map(appeal_status_name))
        .bind(query.appellant_id.as_deref())
        .bind(query.content_id.as_deref())
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(hydrate_appeal)
            .collect::<Result<Vec<_>, RepositoryError>>()
    }

    pub(crate) async fn review_appeal(
        &self,
        appeal_id: &str,
        reviewer_id: &str,
        request: ReviewContentAppealRequest,
    ) -> Result<ContentAppealDto, RepositoryError> {
        let Some(pool) = &self.pool else {
            let mut appeals = self.appeals.write().await;
            let appeal = appeals
                .by_id
                .get_mut(appeal_id)
                .ok_or_else(|| RepositoryError::AppealNotFound(appeal_id.to_string()))?;
            return apply_appeal_review(appeal, reviewer_id, &request);
        };
        let mut tx = pool.begin().await.map_err(RepositoryError::Database)?;
        let row = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM content_appeals WHERE id = $1 FOR UPDATE",
        )
        .bind(appeal_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::AppealNotFound(appeal_id.to_string()))?;
        let mut appeal = hydrate_appeal(row)?;
        if is_appeal_terminal(appeal.status) {
            let reviewed = apply_appeal_review(&mut appeal, reviewer_id, &request)?;
            ensure_appeal_notification_job(&mut tx, &reviewed).await?;
            tx.commit().await.map_err(RepositoryError::Database)?;
            return Ok(reviewed);
        }
        let reviewed = apply_appeal_review(&mut appeal, reviewer_id, &request)?;
        let payload = serde_json::to_value(&reviewed).map_err(RepositoryError::Serialization)?;
        let updated_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "UPDATE content_appeals SET status = $2,assignee_id = $3,resolution = $4,payload = $5,updated_at = now() WHERE id = $1 RETURNING updated_at",
        )
        .bind(appeal_id)
        .bind(appeal_status_name(reviewed.status))
        .bind(&reviewed.assignee_id)
        .bind(&reviewed.resolution)
        .bind(payload)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?;
        ensure_appeal_notification_job(&mut tx, &reviewed).await?;
        tx.commit().await.map_err(RepositoryError::Database)?;
        appeal.updated_at = format_timestamp(updated_at);
        Ok(appeal)
    }
}

async fn ensure_report_restriction_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report: &ContentReportDto,
) -> Result<(), RepositoryError> {
    if report.status != ContentReportStatusDto::Resolved
        || report.action != ContentReportActionDto::RestrictContent
    {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO content_report_restriction_jobs (report_id,content_id) VALUES ($1,$2) ON CONFLICT (report_id) DO NOTHING",
    )
    .bind(&report.id)
    .bind(&report.content_id)
    .execute(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    Ok(())
}

async fn ensure_appeal_notification_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    appeal: &ContentAppealDto,
) -> Result<(), RepositoryError> {
    if !is_appeal_terminal(appeal.status) {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO content_appeal_notification_jobs (appeal_id,user_id,content_id,decision_status,action,resolution) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (appeal_id) DO NOTHING",
    )
    .bind(&appeal.id)
    .bind(&appeal.appellant_id)
    .bind(&appeal.content_id)
    .bind(appeal_status_name(appeal.status))
    .bind(appeal_action_name(appeal.action))
    .bind(appeal.resolution.as_deref().unwrap_or_default())
    .execute(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    Ok(())
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn report_status_name(status: ContentReportStatusDto) -> &'static str {
    match status {
        ContentReportStatusDto::Pending => "pending",
        ContentReportStatusDto::Reviewing => "reviewing",
        ContentReportStatusDto::Resolved => "resolved",
        ContentReportStatusDto::Rejected => "rejected",
    }
}

fn parse_report_status(status: String) -> Result<ContentReportStatusDto, RepositoryError> {
    serde_json::from_value(serde_json::Value::String(status))
        .map_err(RepositoryError::Serialization)
}

fn hydrate_report(row: ModerationRow) -> Result<ContentReportDto, RepositoryError> {
    let (payload, status, assignee_id, resolution, created_at, updated_at) = row;
    let mut report = serde_json::from_value::<ContentReportDto>(payload)
        .map_err(RepositoryError::Serialization)?;
    report.status = parse_report_status(status)?;
    report.assignee_id = assignee_id;
    report.resolution = resolution;
    report.created_at = format_timestamp(created_at);
    report.updated_at = format_timestamp(updated_at);
    Ok(report)
}

fn appeal_status_name(status: ContentAppealStatusDto) -> &'static str {
    match status {
        ContentAppealStatusDto::Pending => "pending",
        ContentAppealStatusDto::Reviewing => "reviewing",
        ContentAppealStatusDto::Resolved => "resolved",
        ContentAppealStatusDto::Rejected => "rejected",
    }
}

fn appeal_action_name(action: ContentReportActionDto) -> &'static str {
    match action {
        ContentReportActionDto::NoAction => "no_action",
        ContentReportActionDto::RestrictContent => "restrict_content",
        ContentReportActionDto::RestoreContent => "restore_content",
    }
}

fn parse_appeal_status(status: String) -> Result<ContentAppealStatusDto, RepositoryError> {
    serde_json::from_value(serde_json::Value::String(status))
        .map_err(RepositoryError::Serialization)
}

fn hydrate_appeal(row: ModerationRow) -> Result<ContentAppealDto, RepositoryError> {
    let (payload, status, assignee_id, resolution, created_at, updated_at) = row;
    let mut appeal = serde_json::from_value::<ContentAppealDto>(payload)
        .map_err(RepositoryError::Serialization)?;
    appeal.status = parse_appeal_status(status)?;
    appeal.assignee_id = assignee_id;
    appeal.resolution = resolution;
    appeal.created_at = format_timestamp(created_at);
    appeal.updated_at = format_timestamp(updated_at);
    Ok(appeal)
}

fn is_terminal(status: ContentReportStatusDto) -> bool {
    matches!(
        status,
        ContentReportStatusDto::Resolved | ContentReportStatusDto::Rejected
    )
}

fn is_appeal_terminal(status: ContentAppealStatusDto) -> bool {
    matches!(
        status,
        ContentAppealStatusDto::Resolved | ContentAppealStatusDto::Rejected
    )
}

fn apply_review(
    report: &mut ContentReportDto,
    reviewer_id: &str,
    request: &ReviewContentReportRequest,
) -> Result<ContentReportDto, RepositoryError> {
    if matches!(request.status, ContentReportStatusDto::Pending) {
        return Err(RepositoryError::InvalidReview(
            "pending is not a human review decision".to_string(),
        ));
    }
    if request.status == ContentReportStatusDto::Reviewing && !request.resolution.is_empty() {
        return Err(RepositoryError::InvalidReview(
            "reviewing reports cannot have a resolution".to_string(),
        ));
    }
    if request.status == ContentReportStatusDto::Reviewing
        && request.action != ContentReportActionDto::NoAction
    {
        return Err(RepositoryError::InvalidReview(
            "reviewing reports cannot change content state".to_string(),
        ));
    }
    if is_terminal(request.status) && request.resolution.is_empty() {
        return Err(RepositoryError::InvalidReview(
            "terminal report decisions require a resolution".to_string(),
        ));
    }
    if request.status == ContentReportStatusDto::Rejected
        && request.action != ContentReportActionDto::NoAction
    {
        return Err(RepositoryError::InvalidReview(
            "rejected reports cannot change content state".to_string(),
        ));
    }
    if request.action == ContentReportActionDto::RestoreContent {
        return Err(RepositoryError::InvalidReview(
            "reports cannot restore content".to_string(),
        ));
    }
    if is_terminal(report.status) {
        return (report.status == request.status
            && report.resolution.as_deref() == Some(request.resolution.as_str())
            && report.action == request.action)
            .then(|| report.clone())
            .ok_or(RepositoryError::ReportConflict);
    }

    report.status = request.status;
    report.assignee_id = Some(reviewer_id.to_string());
    report.resolution = is_terminal(request.status).then(|| request.resolution.clone());
    report.action = request.action;
    report.updated_at = format_timestamp(OffsetDateTime::now_utc());
    Ok(report.clone())
}

fn apply_appeal_review(
    appeal: &mut ContentAppealDto,
    reviewer_id: &str,
    request: &ReviewContentAppealRequest,
) -> Result<ContentAppealDto, RepositoryError> {
    if matches!(request.status, ContentAppealStatusDto::Pending) {
        return Err(RepositoryError::InvalidAppealReview(
            "pending is not a human appeal decision".to_string(),
        ));
    }
    if request.status == ContentAppealStatusDto::Reviewing
        && (!request.resolution.is_empty() || request.action != ContentReportActionDto::NoAction)
    {
        return Err(RepositoryError::InvalidAppealReview(
            "reviewing appeals cannot have a resolution or content action".to_string(),
        ));
    }
    if is_appeal_terminal(request.status) && request.resolution.is_empty() {
        return Err(RepositoryError::InvalidAppealReview(
            "terminal appeal decisions require a resolution".to_string(),
        ));
    }
    if request.status == ContentAppealStatusDto::Rejected
        && request.action != ContentReportActionDto::NoAction
    {
        return Err(RepositoryError::InvalidAppealReview(
            "rejected appeals cannot change content state".to_string(),
        ));
    }
    if request.action == ContentReportActionDto::RestrictContent {
        return Err(RepositoryError::InvalidAppealReview(
            "appeals cannot restrict content".to_string(),
        ));
    }
    if is_appeal_terminal(appeal.status) {
        return (appeal.status == request.status
            && appeal.resolution.as_deref() == Some(request.resolution.as_str())
            && appeal.action == request.action)
            .then(|| appeal.clone())
            .ok_or(RepositoryError::AppealConflict);
    }
    appeal.status = request.status;
    appeal.assignee_id = Some(reviewer_id.to_string());
    appeal.resolution = is_appeal_terminal(request.status).then(|| request.resolution.clone());
    appeal.action = request.action;
    appeal.updated_at = format_timestamp(OffsetDateTime::now_utc());
    Ok(appeal.clone())
}

fn report_reason(reason: bookway_api::ReportReasonDto) -> &'static str {
    match reason {
        bookway_api::ReportReasonDto::Spam => "spam",
        bookway_api::ReportReasonDto::Harassment => "harassment",
        bookway_api::ReportReasonDto::Unsafe => "unsafe",
        bookway_api::ReportReasonDto::Misinformation => "misinformation",
        bookway_api::ReportReasonDto::Copyright => "copyright",
        bookway_api::ReportReasonDto::Privacy => "privacy",
        bookway_api::ReportReasonDto::Other => "other",
    }
}
