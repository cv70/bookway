use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("parent comment {0} was not found on this post")]
    ParentNotFound(String),
    #[error("comment reply nesting exceeds the maximum depth")]
    ReplyDepthExceeded,
    #[error("idempotency key was already used for a different comment")]
    IdempotencyConflict,
    #[error("comment {0} was not found")]
    NotFound(String),
    #[error("stored comment has invalid moderation state {0}")]
    InvalidModerationState(String),
    #[error("stored comment has invalid reply hierarchy")]
    InvalidReplyHierarchy,
    #[error("comment was already decided by another moderator")]
    ModerationConflict,
    #[error("comment {0} is not available for reporting")]
    NotReportable(String),
    #[error("a comment author cannot report their own comment")]
    SelfReport,
    #[error("comment report idempotency key was reused for a different report")]
    ReportIdempotencyConflict,
    #[error("comment report {0} was not found")]
    ReportNotFound(String),
    #[error("comment report is already in a terminal state")]
    ReportConflict,
    #[error("comment appeal idempotency key was reused for a different appeal")]
    AppealIdempotencyConflict,
    #[error("comment appeal {0} was not found")]
    AppealNotFound(String),
    #[error("comment appeal is already in a terminal state")]
    AppealConflict,
    #[error("the requested moderation action no longer applies to this comment")]
    ActionConflict,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

/// A bounded thread avoids unbounded recursive rendering and ancestor scans.
/// Root comments have depth zero, so a thread can contain three reply levels.
pub(crate) const MAX_REPLY_DEPTH: usize = 3;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CommentCursor {
    pub(crate) created_at: OffsetDateTime,
    pub(crate) id: String,
}

impl CommentCursor {
    pub(crate) fn decode(value: &str) -> Option<Self> {
        let (created_at, id) = value.split_once('|')?;
        let created_at = OffsetDateTime::parse(created_at, &Rfc3339).ok()?;
        (!id.is_empty()).then(|| Self {
            created_at,
            id: id.to_string(),
        })
    }

    pub(crate) fn from_comment(comment: &pb::CommentItem) -> Option<Self> {
        Self::from_values(&comment.created_at, &comment.id)
    }

    pub(crate) fn from_values(created_at: &str, id: &str) -> Option<Self> {
        Some(Self {
            created_at: OffsetDateTime::parse(created_at, &Rfc3339).ok()?,
            id: id.to_string(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.created_at), self.id)
    }
}

#[async_trait]
pub(crate) trait CommentDao: Send + Sync {
    async fn get(
        &self,
        post_id: &str,
        comment_id: &str,
        excluded_author_ids: &[String],
    ) -> Result<pb::CommentItem, DaoError>;
    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<pb::CommentItem>, DaoError>;
    async fn list_moderation(
        &self,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentItem>, DaoError>;
    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<pb::CreateCommentResult, DaoError>;
    async fn delete(&self, user_id: &str, post_id: &str, comment_id: &str) -> Result<(), DaoError>;
    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: i32,
    ) -> Result<pb::CommentItem, DaoError>;
    async fn review(
        &self,
        comment_id: &str,
        reviewer_id: &str,
        status: i32,
    ) -> Result<pb::ReviewCommentResult, DaoError>;
    async fn create_report(
        &self,
        input: CreateCommentReportInput,
    ) -> Result<pb::CommentReport, DaoError>;
    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentReport>, DaoError>;
    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewCommentReportInput,
    ) -> Result<pb::CommentReport, DaoError>;
    async fn create_appeal(
        &self,
        input: CreateCommentAppealInput,
    ) -> Result<pb::CommentAppeal, DaoError>;
    async fn list_appeals(
        &self,
        author_id: Option<&str>,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentAppeal>, DaoError>;
    async fn review_appeal(
        &self,
        appeal_id: &str,
        input: ReviewCommentAppealInput,
    ) -> Result<pb::CommentAppeal, DaoError>;
}

pub(crate) struct CreateCommentInput<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) post_id: &'a str,
    pub(crate) author_name: &'a str,
    pub(crate) body: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) excluded_author_ids: &'a [String],
    pub(crate) idempotency_key: Option<String>,
}

pub(crate) struct CreateCommentReportInput {
    pub(crate) id: String,
    pub(crate) reporter_id: String,
    pub(crate) post_id: String,
    pub(crate) comment_id: String,
    pub(crate) idempotency_key: String,
    pub(crate) reason: i32,
    pub(crate) details: String,
    pub(crate) excluded_author_ids: Vec<String>,
    pub(crate) created_at: String,
}

pub(crate) struct ReviewCommentReportInput {
    pub(crate) reviewer_id: String,
    pub(crate) status: i32,
    pub(crate) resolution: String,
    pub(crate) action: i32,
}

pub(crate) struct CreateCommentAppealInput {
    pub(crate) id: String,
    pub(crate) author_id: String,
    pub(crate) comment_id: String,
    pub(crate) idempotency_key: String,
    pub(crate) details: String,
    pub(crate) created_at: String,
}

pub(crate) struct ReviewCommentAppealInput {
    pub(crate) reviewer_id: String,
    pub(crate) status: i32,
    pub(crate) resolution: String,
    pub(crate) action: i32,
}

#[derive(Default)]
struct MemoryCommentState {
    comments: HashMap<String, Vec<pb::CommentItem>>,
    requests: HashMap<(String, String), String>,
    reports: HashMap<String, pb::CommentReport>,
    report_requests: HashMap<(String, String), String>,
    appeals: HashMap<String, pb::CommentAppeal>,
    appeal_requests: HashMap<(String, String), String>,
}

fn public_comment_items(
    items: &[pb::CommentItem],
    excluded_author_ids: &[String],
) -> Vec<pb::CommentItem> {
    let comments_by_id = items
        .iter()
        .map(|comment| (comment.id.as_str(), comment))
        .collect::<HashMap<_, _>>();
    let mut tombstone_ids = HashSet::new();

    for comment in items.iter().filter(|comment| {
        comment.status == pb::CommentStatus::Published as i32
            && !excluded_author_ids.contains(&comment.author_id)
    }) {
        let mut parent_id = comment.parent_id.as_deref();
        let mut visited = HashSet::new();
        while let Some(parent_comment_id) = parent_id {
            if !visited.insert(parent_comment_id) {
                break;
            }
            let Some(parent) = comments_by_id.get(parent_comment_id) else {
                break;
            };
            if parent.status != pb::CommentStatus::Deleted as i32
                || excluded_author_ids.contains(&parent.author_id)
            {
                break;
            }
            tombstone_ids.insert(parent.id.as_str());
            parent_id = parent.parent_id.as_deref();
        }
    }

    items
        .iter()
        .filter_map(|comment| {
            if comment.status == pb::CommentStatus::Published as i32
                && !excluded_author_ids.contains(&comment.author_id)
            {
                Some(comment.clone())
            } else if comment.status == pb::CommentStatus::Deleted as i32
                && !excluded_author_ids.contains(&comment.author_id)
                && tombstone_ids.contains(comment.id.as_str())
            {
                Some(deleted_comment(
                    comment.id.clone(),
                    comment.post_id.clone(),
                    comment.parent_id.clone(),
                    comment.created_at.clone(),
                ))
            } else {
                None
            }
        })
        .collect()
}

async fn select_stored_comment(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    comment_id: &str,
) -> Result<Option<postgres_comment_dao::StoredCommentRow>, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::StoredCommentRow>(
        "SELECT id,post_id,author_id,parent_id,body,like_count,created_at,moderation_state,deleted_at IS NOT NULL FROM comments WHERE id = $1 FOR UPDATE",
    )
    .bind(comment_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)
}

async fn select_comment_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<postgres_comment_dao::CommentReportRow, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::CommentReportRow>(
        "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE r.id = $1",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))
}

async fn select_comment_report_by_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    reporter_id: &str,
    idempotency_key: &str,
) -> Result<Option<postgres_comment_dao::CommentReportRow>, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::CommentReportRow>(
        "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE r.reporter_id = $1 AND r.idempotency_key = $2",
    )
    .bind(reporter_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)
}

async fn select_comment_report_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<postgres_comment_dao::CommentReportRow, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::CommentReportRow>(
        "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE r.id = $1 FOR UPDATE OF r,c",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))
}

async fn select_comment_appeal(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    appeal_id: &str,
) -> Result<postgres_comment_dao::CommentAppealRow, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::CommentAppealRow>(
        "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE a.id = $1",
    )
    .bind(appeal_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::AppealNotFound(appeal_id.to_string()))
}

async fn select_comment_appeal_by_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    author_id: &str,
    idempotency_key: &str,
) -> Result<Option<postgres_comment_dao::CommentAppealRow>, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::CommentAppealRow>(
        "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE a.author_id = $1 AND a.idempotency_key = $2",
    )
    .bind(author_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)
}

async fn select_comment_appeal_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    appeal_id: &str,
) -> Result<postgres_comment_dao::CommentAppealRow, DaoError> {
    sqlx::query_as::<_, postgres_comment_dao::CommentAppealRow>(
        "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE a.id = $1 FOR UPDATE OF a,c",
    )
    .bind(appeal_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::AppealNotFound(appeal_id.to_string()))
}

fn memory_comment(state: &MemoryCommentState, comment_id: &str) -> Option<pb::CommentItem> {
    state
        .comments
        .values()
        .flatten()
        .find(|comment| comment.id == comment_id)
        .cloned()
}

fn memory_comment_mut<'a>(
    state: &'a mut MemoryCommentState,
    comment_id: &str,
) -> Option<&'a mut pb::CommentItem> {
    state
        .comments
        .values_mut()
        .flatten()
        .find(|comment| comment.id == comment_id)
}

fn update_memory_comment_records(state: &mut MemoryCommentState, comment: &pb::CommentItem) {
    for report in state.reports.values_mut() {
        if report
            .reported_comment
            .as_ref()
            .is_some_and(|reported_comment| reported_comment.id == comment.id)
        {
            report.reported_comment = Some(comment.clone());
        }
    }
    for appeal in state.appeals.values_mut() {
        if appeal
            .appealed_comment
            .as_ref()
            .is_some_and(|appealed_comment| appealed_comment.id == comment.id)
        {
            appeal.appealed_comment = Some(comment.clone());
        }
    }
}

fn is_terminal_report(status: i32) -> bool {
    matches!(
        pb::CommentReportStatus::try_from(status),
        Ok(pb::CommentReportStatus::Resolved | pb::CommentReportStatus::Rejected)
    )
}

fn is_terminal_appeal(status: i32) -> bool {
    matches!(
        pb::CommentAppealStatus::try_from(status),
        Ok(pb::CommentAppealStatus::Resolved | pb::CommentAppealStatus::Rejected)
    )
}

fn apply_report_review(
    report: &mut pb::CommentReport,
    input: &ReviewCommentReportInput,
) -> Result<pb::CommentReport, DaoError> {
    let status = pb::CommentReportStatus::try_from(input.status).map_err(|_| {
        DaoError::InvalidModerationState("unknown comment report status".to_string())
    })?;
    let action = pb::CommentReportAction::try_from(input.action).map_err(|_| {
        DaoError::InvalidModerationState("unknown comment report action".to_string())
    })?;
    if is_terminal_report(report.status) {
        return (report.status == input.status
            && report.resolution.as_deref() == Some(input.resolution.as_str())
            && report.action == input.action)
            .then(|| report.clone())
            .ok_or(DaoError::ReportConflict);
    }
    if status == pb::CommentReportStatus::Pending {
        return Err(DaoError::InvalidModerationState(
            "pending is not a review decision".to_string(),
        ));
    }
    if status == pb::CommentReportStatus::Reviewing
        && (!input.resolution.is_empty() || action != pb::CommentReportAction::NoAction)
    {
        return Err(DaoError::InvalidModerationState(
            "reviewing reports cannot resolve or restrict a comment".to_string(),
        ));
    }
    if is_terminal_report(input.status) && input.resolution.is_empty() {
        return Err(DaoError::InvalidModerationState(
            "terminal reviews require a resolution".to_string(),
        ));
    }
    if status == pb::CommentReportStatus::Rejected && action != pb::CommentReportAction::NoAction {
        return Err(DaoError::InvalidModerationState(
            "rejected reports cannot restrict a comment".to_string(),
        ));
    }
    report.status = input.status;
    report.reviewer_id = Some(input.reviewer_id.clone());
    report.resolution = is_terminal_report(input.status).then(|| input.resolution.clone());
    report.action = input.action;
    report.updated_at = now_timestamp();
    Ok(report.clone())
}

fn apply_appeal_review(
    appeal: &mut pb::CommentAppeal,
    input: &ReviewCommentAppealInput,
) -> Result<pb::CommentAppeal, DaoError> {
    let status = pb::CommentAppealStatus::try_from(input.status).map_err(|_| {
        DaoError::InvalidModerationState("unknown comment appeal status".to_string())
    })?;
    let action = pb::CommentAppealAction::try_from(input.action).map_err(|_| {
        DaoError::InvalidModerationState("unknown comment appeal action".to_string())
    })?;
    if is_terminal_appeal(appeal.status) {
        return (appeal.status == input.status
            && appeal.resolution.as_deref() == Some(input.resolution.as_str())
            && appeal.action == input.action)
            .then(|| appeal.clone())
            .ok_or(DaoError::AppealConflict);
    }
    if status == pb::CommentAppealStatus::Pending {
        return Err(DaoError::InvalidModerationState(
            "pending is not a review decision".to_string(),
        ));
    }
    if status == pb::CommentAppealStatus::Reviewing
        && (!input.resolution.is_empty() || action != pb::CommentAppealAction::NoAction)
    {
        return Err(DaoError::InvalidModerationState(
            "reviewing appeals cannot resolve or restore a comment".to_string(),
        ));
    }
    if is_terminal_appeal(input.status) && input.resolution.is_empty() {
        return Err(DaoError::InvalidModerationState(
            "terminal appeal reviews require a resolution".to_string(),
        ));
    }
    if status == pb::CommentAppealStatus::Rejected && action != pb::CommentAppealAction::NoAction {
        return Err(DaoError::InvalidModerationState(
            "rejected appeals cannot restore a comment".to_string(),
        ));
    }
    appeal.status = input.status;
    appeal.reviewer_id = Some(input.reviewer_id.clone());
    appeal.resolution = is_terminal_appeal(input.status).then(|| input.resolution.clone());
    appeal.action = input.action;
    appeal.updated_at = now_timestamp();
    Ok(appeal.clone())
}

fn memory_parent_author_id(state: &MemoryCommentState, parent_id: Option<&str>) -> Option<String> {
    let parent_id = parent_id?;
    state
        .comments
        .values()
        .flatten()
        .find(|comment| comment.id == parent_id)
        .map(|comment| comment.author_id.clone())
}

fn memory_comment_depth(
    items: &[pb::CommentItem],
    start: &pb::CommentItem,
) -> Result<usize, DaoError> {
    let mut current = start;
    let mut depth = 0;
    while let Some(parent_id) = current.parent_id.as_deref() {
        if depth >= MAX_REPLY_DEPTH {
            return Ok(depth);
        }
        depth += 1;
        current = items
            .iter()
            .find(|comment| comment.id == parent_id)
            .ok_or_else(|| DaoError::ParentNotFound(parent_id.to_string()))?;
    }
    Ok(depth)
}

async fn find_parent_author_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    parent_id: Option<&str>,
) -> Result<Option<String>, DaoError> {
    let Some(parent_id) = parent_id else {
        return Ok(None);
    };
    sqlx::query_scalar::<_, String>("SELECT author_id FROM comments WHERE id = $1")
        .bind(parent_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(DaoError::Database)
}

async fn find_idempotent_comment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    key: &str,
) -> Result<Option<IdempotentComment>, DaoError> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            i64,
            time::OffsetDateTime,
            String,
            bool,
        ),
    >(
        "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted_at IS NOT NULL FROM comments WHERE author_id = $1 AND client_request_id = $2",
    )
    .bind(user_id)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(DaoError::Database)?;
    row.map(
        |(
            id,
            post_id,
            author_id,
            parent_id,
            body,
            like_count,
            created_at,
            moderation_state,
            deleted,
        )| {
            let request_body = body.clone();
            let comment = comment_from_stored_row((
                id,
                post_id,
                author_id,
                parent_id,
                body,
                like_count,
                created_at,
                moderation_state,
                deleted,
            ))?;
            Ok(IdempotentComment {
                comment,
                request_body,
            })
        },
    )
    .transpose()
}

struct IdempotentComment {
    comment: pb::CommentItem,
    request_body: String,
}

fn comment_from_stored_row(
    (id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted): (
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        time::OffsetDateTime,
        String,
        bool,
    ),
) -> Result<pb::CommentItem, DaoError> {
    if deleted {
        return Ok(deleted_comment(
            id,
            post_id,
            parent_id,
            format_timestamp(created_at),
        ));
    }
    comment_from_row((
        id,
        post_id,
        author_id,
        parent_id,
        body,
        like_count,
        created_at,
        moderation_state,
    ))
}

// Moderators need the original context even after a user deletes the comment;
// public list rendering continues to use the redacted helper above.
fn moderation_comment_from_stored_row(
    (id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted): (
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        time::OffsetDateTime,
        String,
        bool,
    ),
) -> Result<pb::CommentItem, DaoError> {
    let mut comment = comment_from_row((
        id,
        post_id,
        author_id,
        parent_id,
        body,
        like_count,
        created_at,
        moderation_state,
    ))?;
    if deleted {
        comment.status = pb::CommentStatus::Deleted as i32;
    }
    Ok(comment)
}

fn deleted_comment(
    id: String,
    post_id: String,
    parent_id: Option<String>,
    created_at: String,
) -> pb::CommentItem {
    pb::CommentItem {
        id,
        post_id,
        author_id: String::new(),
        author_name: "已删除用户".to_string(),
        body: "该评论已删除".to_string(),
        parent_id,
        like_count: 0,
        created_at,
        status: pb::CommentStatus::Deleted as i32,
    }
}

fn comment_from_row(
    (id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state): (
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        time::OffsetDateTime,
        String,
    ),
) -> Result<pb::CommentItem, DaoError> {
    Ok(pb::CommentItem {
        id,
        post_id,
        author_name: author_id.clone(),
        author_id,
        body,
        parent_id,
        like_count: like_count.max(0) as u64,
        created_at: format_timestamp(created_at),
        status: moderation_status(&moderation_state)?,
    })
}

fn moderation_status(value: &str) -> Result<i32, DaoError> {
    match value {
        "reviewing" => Ok(pb::CommentStatus::Reviewing as i32),
        "published" => Ok(pb::CommentStatus::Published as i32),
        "restricted" => Ok(pb::CommentStatus::Restricted as i32),
        value => Err(DaoError::InvalidModerationState(value.to_string())),
    }
}

fn moderation_state_name(value: i32) -> Option<&'static str> {
    match pb::CommentStatus::try_from(value) {
        Ok(pb::CommentStatus::Reviewing) => Some("reviewing"),
        Ok(pb::CommentStatus::Published) => Some("published"),
        Ok(pb::CommentStatus::Restricted) => Some("restricted"),
        Ok(pb::CommentStatus::Deleted) | Err(_) => None,
    }
}

fn comment_report_reason_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::CommentReportReason::try_from(value) {
        Ok(pb::CommentReportReason::Spam) => Ok("spam"),
        Ok(pb::CommentReportReason::Harassment) => Ok("harassment"),
        Ok(pb::CommentReportReason::Unsafe) => Ok("unsafe"),
        Ok(pb::CommentReportReason::Fraud) => Ok("fraud"),
        Ok(pb::CommentReportReason::Privacy) => Ok("privacy"),
        Ok(pb::CommentReportReason::Other) => Ok("other"),
        Ok(pb::CommentReportReason::Unspecified) | Err(_) => Err(DaoError::InvalidModerationState(
            "unknown comment report reason".to_string(),
        )),
    }
}

fn parse_comment_report_reason(value: &str) -> Result<i32, DaoError> {
    let reason = match value {
        "spam" => pb::CommentReportReason::Spam,
        "harassment" => pb::CommentReportReason::Harassment,
        "unsafe" => pb::CommentReportReason::Unsafe,
        "fraud" => pb::CommentReportReason::Fraud,
        "privacy" => pb::CommentReportReason::Privacy,
        "other" => pb::CommentReportReason::Other,
        _ => {
            return Err(DaoError::InvalidModerationState(format!(
                "unknown comment report reason {value}"
            )));
        }
    };
    Ok(reason as i32)
}

fn comment_report_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::CommentReportStatus::try_from(value) {
        Ok(pb::CommentReportStatus::Pending) => Ok("pending"),
        Ok(pb::CommentReportStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::CommentReportStatus::Resolved) => Ok("resolved"),
        Ok(pb::CommentReportStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(DaoError::InvalidModerationState(format!(
            "unknown comment report status {value}"
        ))),
    }
}

fn parse_comment_report_status(value: &str) -> Result<i32, DaoError> {
    let status = match value {
        "pending" => pb::CommentReportStatus::Pending,
        "reviewing" => pb::CommentReportStatus::Reviewing,
        "resolved" => pb::CommentReportStatus::Resolved,
        "rejected" => pb::CommentReportStatus::Rejected,
        _ => {
            return Err(DaoError::InvalidModerationState(format!(
                "unknown comment report status {value}"
            )));
        }
    };
    Ok(status as i32)
}

fn comment_report_action_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::CommentReportAction::try_from(value) {
        Ok(pb::CommentReportAction::NoAction) => Ok("no_action"),
        Ok(pb::CommentReportAction::RestrictComment) => Ok("restrict_comment"),
        Err(_) => Err(DaoError::InvalidModerationState(format!(
            "unknown comment report action {value}"
        ))),
    }
}

fn parse_comment_report_action(value: &str) -> Result<i32, DaoError> {
    let action = match value {
        "no_action" => pb::CommentReportAction::NoAction,
        "restrict_comment" => pb::CommentReportAction::RestrictComment,
        _ => {
            return Err(DaoError::InvalidModerationState(format!(
                "unknown comment report action {value}"
            )));
        }
    };
    Ok(action as i32)
}

fn comment_appeal_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::CommentAppealStatus::try_from(value) {
        Ok(pb::CommentAppealStatus::Pending) => Ok("pending"),
        Ok(pb::CommentAppealStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::CommentAppealStatus::Resolved) => Ok("resolved"),
        Ok(pb::CommentAppealStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(DaoError::InvalidModerationState(format!(
            "unknown comment appeal status {value}"
        ))),
    }
}

fn parse_comment_appeal_status(value: &str) -> Result<i32, DaoError> {
    let status = match value {
        "pending" => pb::CommentAppealStatus::Pending,
        "reviewing" => pb::CommentAppealStatus::Reviewing,
        "resolved" => pb::CommentAppealStatus::Resolved,
        "rejected" => pb::CommentAppealStatus::Rejected,
        _ => {
            return Err(DaoError::InvalidModerationState(format!(
                "unknown comment appeal status {value}"
            )));
        }
    };
    Ok(status as i32)
}

fn comment_appeal_action_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::CommentAppealAction::try_from(value) {
        Ok(pb::CommentAppealAction::NoAction) => Ok("no_action"),
        Ok(pb::CommentAppealAction::RestoreComment) => Ok("restore_comment"),
        Err(_) => Err(DaoError::InvalidModerationState(format!(
            "unknown comment appeal action {value}"
        ))),
    }
}

fn parse_comment_appeal_action(value: &str) -> Result<i32, DaoError> {
    let action = match value {
        "no_action" => pb::CommentAppealAction::NoAction,
        "restore_comment" => pb::CommentAppealAction::RestoreComment,
        _ => {
            return Err(DaoError::InvalidModerationState(format!(
                "unknown comment appeal action {value}"
            )));
        }
    };
    Ok(action as i32)
}

fn now_timestamp() -> String {
    format_timestamp(OffsetDateTime::now_utc())
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn published_comment(dao: &MemoryCommentDao) -> pb::CommentItem {
        let comment = dao
            .create(CreateCommentInput {
                user_id: "author",
                post_id: "post",
                author_name: "author",
                body: "visible comment".to_string(),
                parent_id: None,
                excluded_author_ids: &[],
                idempotency_key: Some("comment-1".to_string()),
            })
            .await
            .expect("create comment")
            .comment
            .expect("comment result");
        dao.set_moderation_status(&comment.id, pb::CommentStatus::Published as i32)
            .await
            .expect("publish comment")
    }

    fn report_input(comment_id: &str, details: &str) -> CreateCommentReportInput {
        CreateCommentReportInput {
            id: uuid::Uuid::now_v7().to_string(),
            reporter_id: "reader".to_string(),
            post_id: "post".to_string(),
            comment_id: comment_id.to_string(),
            idempotency_key: "report-1".to_string(),
            reason: pb::CommentReportReason::Harassment as i32,
            details: details.to_string(),
            excluded_author_ids: Vec::new(),
            created_at: now_timestamp(),
        }
    }

    #[tokio::test]
    async fn report_restriction_and_appeal_restore_public_visibility() {
        let dao = MemoryCommentDao::default();
        let comment = published_comment(&dao).await;
        let report = dao
            .create_report(report_input(&comment.id, "repeated harassment"))
            .await
            .expect("report published comment");
        let retry = dao
            .create_report(report_input(&comment.id, "repeated harassment"))
            .await
            .expect("idempotent report retry");
        let conflicting_retry = dao
            .create_report(report_input(&comment.id, "different details"))
            .await;
        let own_report = dao
            .create_report(CreateCommentReportInput {
                reporter_id: "author".to_string(),
                idempotency_key: "report-own".to_string(),
                ..report_input(&comment.id, "self report")
            })
            .await;
        let hidden_report = dao
            .create_report(CreateCommentReportInput {
                idempotency_key: "report-hidden".to_string(),
                excluded_author_ids: vec!["author".to_string()],
                ..report_input(&comment.id, "hidden report")
            })
            .await;

        assert_eq!(report.id, retry.id);
        assert!(matches!(
            conflicting_retry,
            Err(DaoError::ReportIdempotencyConflict)
        ));
        assert!(matches!(own_report, Err(DaoError::SelfReport)));
        assert!(matches!(hidden_report, Err(DaoError::NotReportable(_))));

        let restricted = dao
            .review_report(
                &report.id,
                ReviewCommentReportInput {
                    reviewer_id: "moderator".to_string(),
                    status: pb::CommentReportStatus::Resolved as i32,
                    resolution: "violates community safety rules".to_string(),
                    action: pb::CommentReportAction::RestrictComment as i32,
                },
            )
            .await
            .expect("restrict comment");
        assert_eq!(
            restricted
                .reported_comment
                .as_ref()
                .expect("reported comment")
                .status,
            pb::CommentStatus::Restricted as i32
        );
        assert!(
            dao.list("post", None, 10, &[])
                .await
                .expect("public comments after restriction")
                .is_empty()
        );

        let appeal = dao
            .create_appeal(CreateCommentAppealInput {
                id: uuid::Uuid::now_v7().to_string(),
                author_id: "author".to_string(),
                comment_id: comment.id.clone(),
                idempotency_key: "appeal-1".to_string(),
                details: "please review the context".to_string(),
                created_at: now_timestamp(),
            })
            .await
            .expect("author appeal");
        let restored = dao
            .review_appeal(
                &appeal.id,
                ReviewCommentAppealInput {
                    reviewer_id: "moderator".to_string(),
                    status: pb::CommentAppealStatus::Resolved as i32,
                    resolution: "context supports restoration".to_string(),
                    action: pb::CommentAppealAction::RestoreComment as i32,
                },
            )
            .await
            .expect("restore comment");
        let conflicting_terminal_review = dao
            .review_report(
                &report.id,
                ReviewCommentReportInput {
                    reviewer_id: "another moderator".to_string(),
                    status: pb::CommentReportStatus::Rejected as i32,
                    resolution: "different outcome".to_string(),
                    action: pb::CommentReportAction::NoAction as i32,
                },
            )
            .await;

        assert_eq!(
            restored
                .appealed_comment
                .as_ref()
                .expect("appealed comment")
                .status,
            pb::CommentStatus::Published as i32
        );
        assert_eq!(
            dao.list("post", None, 10, &[])
                .await
                .expect("public comments after restore")
                .len(),
            1
        );
        assert!(matches!(
            conflicting_terminal_review,
            Err(DaoError::ReportConflict)
        ));
    }

    #[tokio::test]
    async fn get_returns_only_a_visible_published_comment_on_the_requested_post() {
        let dao = MemoryCommentDao::default();
        let comment = published_comment(&dao).await;

        let fetched = dao
            .get("post", &comment.id, &[])
            .await
            .expect("published answer is readable");
        assert_eq!(fetched.id, comment.id);
        assert!(matches!(
            dao.get("post", &comment.id, &["author".to_string()]).await,
            Err(DaoError::NotFound(_))
        ));
        assert!(matches!(
            dao.get("other-post", &comment.id, &[]).await,
            Err(DaoError::NotFound(_))
        ));
    }
}

#[path = "memory_comment_dao.rs"]
mod memory_comment_dao;
pub(crate) use memory_comment_dao::MemoryCommentDao;
#[path = "postgres_comment_dao.rs"]
mod postgres_comment_dao;
pub(crate) use postgres_comment_dao::PostgresCommentDao;
