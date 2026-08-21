use std::{collections::HashMap, sync::Arc};

use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
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
    #[error("stored moderation record is invalid: {0}")]
    InvalidValue(String),
    #[error("stored report is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
}

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

    pub(crate) fn from_report(report: &pb::ContentReport) -> Option<Self> {
        Self::from_values(&report.created_at, &report.id)
    }

    pub(crate) fn from_appeal(appeal: &pb::ContentAppeal) -> Option<Self> {
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

#[derive(Default)]
struct MemoryReports {
    by_id: HashMap<String, pb::ContentReport>,
    idempotency: HashMap<(String, String), String>,
}

#[derive(Default)]
struct MemoryAppeals {
    by_id: HashMap<String, pb::ContentAppeal>,
    idempotency: HashMap<(String, String), String>,
}

async fn ensure_report_restriction_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report: &pb::ContentReport,
) -> Result<(), DaoError> {
    if report.status != pb::ReportStatus::Resolved as i32
        || report.action != pb::ContentAction::Restrict as i32
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
    .map_err(DaoError::Database)?;
    Ok(())
}

async fn ensure_appeal_notification_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    appeal: &pb::ContentAppeal,
) -> Result<(), DaoError> {
    if !is_appeal_terminal(appeal.status) {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO content_appeal_notification_jobs (appeal_id,user_id,content_id,decision_status,action,resolution) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (appeal_id) DO NOTHING",
    )
    .bind(&appeal.id)
    .bind(&appeal.appellant_id)
    .bind(&appeal.content_id)
    .bind(appeal_status_name(appeal.status)?)
    .bind(content_action_name(appeal.action)?)
    .bind(appeal.resolution.as_deref().unwrap_or_default())
    .execute(&mut **transaction)
    .await
    .map_err(DaoError::Database)?;
    Ok(())
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn audit_decision_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::AuditDecision::try_from(value) {
        Ok(pb::AuditDecision::Approved) => Ok("approved"),
        Ok(pb::AuditDecision::Reviewing) => Ok("reviewing"),
        Ok(pb::AuditDecision::Restricted) => Ok("restricted"),
        Err(_) => Err(DaoError::InvalidValue("unknown audit decision".to_string())),
    }
}

fn report_reason_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::ReportReason::try_from(value) {
        Ok(pb::ReportReason::Spam) => Ok("spam"),
        Ok(pb::ReportReason::Harassment) => Ok("harassment"),
        Ok(pb::ReportReason::Unsafe) => Ok("unsafe"),
        Ok(pb::ReportReason::Misinformation) => Ok("misinformation"),
        Ok(pb::ReportReason::Copyright) => Ok("copyright"),
        Ok(pb::ReportReason::Privacy) => Ok("privacy"),
        Ok(pb::ReportReason::Other) => Ok("other"),
        Err(_) => Err(DaoError::InvalidValue("unknown report reason".to_string())),
    }
}

fn report_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::ReportStatus::try_from(value) {
        Ok(pb::ReportStatus::Pending) => Ok("pending"),
        Ok(pb::ReportStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::ReportStatus::Resolved) => Ok("resolved"),
        Ok(pb::ReportStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(DaoError::InvalidValue("unknown report status".to_string())),
    }
}

fn parse_report_status(value: String) -> Result<i32, DaoError> {
    match value.as_str() {
        "pending" => Ok(pb::ReportStatus::Pending as i32),
        "reviewing" => Ok(pb::ReportStatus::Reviewing as i32),
        "resolved" => Ok(pb::ReportStatus::Resolved as i32),
        "rejected" => Ok(pb::ReportStatus::Rejected as i32),
        _ => Err(DaoError::InvalidValue("unknown report status".to_string())),
    }
}

fn appeal_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::AppealStatus::try_from(value) {
        Ok(pb::AppealStatus::Pending) => Ok("pending"),
        Ok(pb::AppealStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::AppealStatus::Resolved) => Ok("resolved"),
        Ok(pb::AppealStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(DaoError::InvalidValue("unknown appeal status".to_string())),
    }
}

fn parse_appeal_status(value: String) -> Result<i32, DaoError> {
    match value.as_str() {
        "pending" => Ok(pb::AppealStatus::Pending as i32),
        "reviewing" => Ok(pb::AppealStatus::Reviewing as i32),
        "resolved" => Ok(pb::AppealStatus::Resolved as i32),
        "rejected" => Ok(pb::AppealStatus::Rejected as i32),
        _ => Err(DaoError::InvalidValue("unknown appeal status".to_string())),
    }
}

fn content_action_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::ContentAction::try_from(value) {
        Ok(pb::ContentAction::NoAction) => Ok("no_action"),
        Ok(pb::ContentAction::Restrict) => Ok("restrict_content"),
        Ok(pb::ContentAction::Restore) => Ok("restore_content"),
        Err(_) => Err(DaoError::InvalidValue("unknown content action".to_string())),
    }
}

fn is_terminal(status: i32) -> bool {
    matches!(
        pb::ReportStatus::try_from(status),
        Ok(pb::ReportStatus::Resolved | pb::ReportStatus::Rejected)
    )
}

fn is_appeal_terminal(status: i32) -> bool {
    matches!(
        pb::AppealStatus::try_from(status),
        Ok(pb::AppealStatus::Resolved | pb::AppealStatus::Rejected)
    )
}

fn apply_review(
    report: &mut pb::ContentReport,
    reviewer_id: &str,
    request: &pb::ReviewReportRequest,
) -> Result<pb::ContentReport, DaoError> {
    let status = pb::ReportStatus::try_from(request.status)
        .map_err(|_| DaoError::InvalidReview("unknown report status".to_string()))?;
    let action = pb::ContentAction::try_from(request.action)
        .map_err(|_| DaoError::InvalidReview("unknown content action".to_string()))?;
    if status == pb::ReportStatus::Pending {
        return Err(DaoError::InvalidReview(
            "pending is not a human review decision".to_string(),
        ));
    }
    if status == pb::ReportStatus::Reviewing && !request.resolution.is_empty() {
        return Err(DaoError::InvalidReview(
            "reviewing reports cannot have a resolution".to_string(),
        ));
    }
    if status == pb::ReportStatus::Reviewing && action != pb::ContentAction::NoAction {
        return Err(DaoError::InvalidReview(
            "reviewing reports cannot change content state".to_string(),
        ));
    }
    if is_terminal(request.status) && request.resolution.is_empty() {
        return Err(DaoError::InvalidReview(
            "terminal report decisions require a resolution".to_string(),
        ));
    }
    if status == pb::ReportStatus::Rejected && action != pb::ContentAction::NoAction {
        return Err(DaoError::InvalidReview(
            "rejected reports cannot change content state".to_string(),
        ));
    }
    if action == pb::ContentAction::Restore {
        return Err(DaoError::InvalidReview(
            "reports cannot restore content".to_string(),
        ));
    }
    if is_terminal(report.status) {
        return (report.status == request.status
            && report.resolution.as_deref() == Some(request.resolution.as_str())
            && report.action == request.action)
            .then(|| report.clone())
            .ok_or(DaoError::ReportConflict);
    }
    report.status = request.status;
    report.assignee_id = Some(reviewer_id.to_string());
    report.resolution = is_terminal(request.status).then(|| request.resolution.clone());
    report.action = request.action;
    report.updated_at = format_timestamp(OffsetDateTime::now_utc());
    Ok(report.clone())
}

fn apply_appeal_review(
    appeal: &mut pb::ContentAppeal,
    reviewer_id: &str,
    request: &pb::ReviewAppealRequest,
) -> Result<pb::ContentAppeal, DaoError> {
    let status = pb::AppealStatus::try_from(request.status)
        .map_err(|_| DaoError::InvalidAppealReview("unknown appeal status".to_string()))?;
    let action = pb::ContentAction::try_from(request.action)
        .map_err(|_| DaoError::InvalidAppealReview("unknown content action".to_string()))?;
    if status == pb::AppealStatus::Pending {
        return Err(DaoError::InvalidAppealReview(
            "pending is not a human appeal decision".to_string(),
        ));
    }
    if status == pb::AppealStatus::Reviewing
        && (!request.resolution.is_empty() || action != pb::ContentAction::NoAction)
    {
        return Err(DaoError::InvalidAppealReview(
            "reviewing appeals cannot have a resolution or content action".to_string(),
        ));
    }
    if is_appeal_terminal(request.status) && request.resolution.is_empty() {
        return Err(DaoError::InvalidAppealReview(
            "terminal appeal decisions require a resolution".to_string(),
        ));
    }
    if status == pb::AppealStatus::Rejected && action != pb::ContentAction::NoAction {
        return Err(DaoError::InvalidAppealReview(
            "rejected appeals cannot change content state".to_string(),
        ));
    }
    if action == pb::ContentAction::Restrict {
        return Err(DaoError::InvalidAppealReview(
            "appeals cannot restrict content".to_string(),
        ));
    }
    if is_appeal_terminal(appeal.status) {
        return (appeal.status == request.status
            && appeal.resolution.as_deref() == Some(request.resolution.as_str())
            && appeal.action == request.action)
            .then(|| appeal.clone())
            .ok_or(DaoError::AppealConflict);
    }
    appeal.status = request.status;
    appeal.assignee_id = Some(reviewer_id.to_string());
    appeal.resolution = is_appeal_terminal(request.status).then(|| request.resolution.clone());
    appeal.action = request.action;
    appeal.updated_at = format_timestamp(OffsetDateTime::now_utc());
    Ok(appeal.clone())
}

#[path = "audit_dao.rs"]
mod audit_dao;
pub(crate) use audit_dao::AuditDao;
