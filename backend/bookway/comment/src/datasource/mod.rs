use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
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
pub(crate) trait CommentRepository: Send + Sync {
    async fn get(
        &self,
        post_id: &str,
        comment_id: &str,
        excluded_author_ids: &[String],
    ) -> Result<pb::CommentItem, RepositoryError>;
    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<pb::CommentItem>, RepositoryError>;
    async fn list_moderation(
        &self,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentItem>, RepositoryError>;
    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<pb::CreateCommentResult, RepositoryError>;
    async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), RepositoryError>;
    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: i32,
    ) -> Result<pb::CommentItem, RepositoryError>;
    async fn review(
        &self,
        comment_id: &str,
        reviewer_id: &str,
        status: i32,
    ) -> Result<pb::ReviewCommentResult, RepositoryError>;
    async fn create_report(
        &self,
        input: CreateCommentReportInput,
    ) -> Result<pb::CommentReport, RepositoryError>;
    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentReport>, RepositoryError>;
    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewCommentReportInput,
    ) -> Result<pb::CommentReport, RepositoryError>;
    async fn create_appeal(
        &self,
        input: CreateCommentAppealInput,
    ) -> Result<pb::CommentAppeal, RepositoryError>;
    async fn list_appeals(
        &self,
        author_id: Option<&str>,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentAppeal>, RepositoryError>;
    async fn review_appeal(
        &self,
        appeal_id: &str,
        input: ReviewCommentAppealInput,
    ) -> Result<pb::CommentAppeal, RepositoryError>;
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
pub(crate) struct MemoryCommentRepository {
    state: RwLock<MemoryCommentState>,
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

#[async_trait]
impl CommentRepository for MemoryCommentRepository {
    async fn get(
        &self,
        post_id: &str,
        comment_id: &str,
        excluded_author_ids: &[String],
    ) -> Result<pb::CommentItem, RepositoryError> {
        let state = self.state.read().await;
        let items = state
            .comments
            .get(post_id)
            .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        public_comment_items(items, excluded_author_ids)
            .into_iter()
            .find(|comment| comment.id == comment_id)
            .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))
    }

    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<pb::CommentItem>, RepositoryError> {
        let mut items = self
            .state
            .read()
            .await
            .comments
            .get(post_id)
            .cloned()
            .unwrap_or_default();
        items = public_comment_items(&items, excluded_author_ids);
        items.sort_by_key(CommentCursor::from_comment);
        if let Some(cursor) = cursor {
            items.retain(|comment| {
                CommentCursor::from_comment(comment).is_some_and(|value| value > cursor.clone())
            });
        }
        items.truncate(limit);
        Ok(items)
    }

    async fn list_moderation(
        &self,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentItem>, RepositoryError> {
        let state = self.state.read().await;
        let mut items = state
            .comments
            .values()
            .flatten()
            .filter(|comment| comment.status == pb::CommentStatus::Reviewing as i32)
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by_key(CommentCursor::from_comment);
        if let Some(cursor) = cursor {
            items.retain(|comment| {
                CommentCursor::from_comment(comment).is_some_and(|value| value > cursor.clone())
            });
        }
        items.truncate(limit);
        Ok(items)
    }

    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<pb::CreateCommentResult, RepositoryError> {
        let CreateCommentInput {
            user_id,
            post_id,
            author_name,
            body,
            parent_id,
            excluded_author_ids,
            idempotency_key,
        } = input;
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key.as_deref()
            && let Some(comment_id) = state.requests.get(&(user_id.to_string(), key.to_string()))
        {
            let existing = state
                .comments
                .values()
                .flatten()
                .find(|comment| comment.id == *comment_id)
                .cloned()
                .ok_or(RepositoryError::IdempotencyConflict)?;
            if existing.post_id == post_id
                && existing.body == body
                && existing.parent_id == parent_id
            {
                return Ok(pb::CreateCommentResult {
                    parent_author_id: memory_parent_author_id(
                        &state,
                        existing.parent_id.as_deref(),
                    ),
                    comment: Some(existing),
                });
            }
            return Err(RepositoryError::IdempotencyConflict);
        }
        let parent_author_id = if let Some(parent_id) = parent_id.as_deref() {
            let items = state
                .comments
                .get(post_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let parent = items
                .iter()
                .find(|comment| {
                    comment.id == parent_id
                        && comment.status == pb::CommentStatus::Published as i32
                        && !excluded_author_ids.contains(&comment.author_id)
                })
                .ok_or_else(|| RepositoryError::ParentNotFound(parent_id.to_string()))?;
            if memory_comment_depth(items, parent)? >= MAX_REPLY_DEPTH {
                return Err(RepositoryError::ReplyDepthExceeded);
            }
            Some(parent.author_id.clone())
        } else {
            None
        };
        let comment = pb::CommentItem {
            id: uuid::Uuid::now_v7().to_string(),
            post_id: post_id.to_string(),
            author_id: user_id.to_string(),
            author_name: author_name.to_string(),
            body,
            parent_id,
            like_count: 0,
            created_at: now_timestamp(),
            status: pb::CommentStatus::Reviewing as i32,
        };
        state
            .comments
            .entry(post_id.to_string())
            .or_default()
            .push(comment.clone());
        if let Some(key) = idempotency_key {
            state
                .requests
                .insert((user_id.to_string(), key), comment.id.clone());
        }
        Ok(pb::CreateCommentResult {
            comment: Some(comment),
            parent_author_id,
        })
    }

    async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.write().await;
        let comment = {
            let comment = state
                .comments
                .get_mut(post_id)
                .into_iter()
                .flatten()
                .find(|comment| comment.id == comment_id && comment.author_id == user_id)
                .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
            comment.status = pb::CommentStatus::Deleted as i32;
            comment.clone()
        };
        update_memory_comment_records(&mut state, &comment);
        Ok(())
    }

    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: i32,
    ) -> Result<pb::CommentItem, RepositoryError> {
        if moderation_state_name(status).is_none() {
            return Err(RepositoryError::InvalidModerationState(format!(
                "{status:?}"
            )));
        }
        let mut state = self.state.write().await;
        let comment = state
            .comments
            .values_mut()
            .flatten()
            .find(|comment| comment.id == comment_id)
            .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        if comment.status == pb::CommentStatus::Reviewing as i32 {
            comment.status = status;
        }
        Ok(comment.clone())
    }

    async fn review(
        &self,
        comment_id: &str,
        _reviewer_id: &str,
        status: i32,
    ) -> Result<pb::ReviewCommentResult, RepositoryError> {
        if !matches!(
            pb::CommentStatus::try_from(status),
            Ok(pb::CommentStatus::Published | pb::CommentStatus::Restricted)
        ) {
            return Err(RepositoryError::InvalidModerationState(format!(
                "{status:?}"
            )));
        }
        let mut state = self.state.write().await;
        let (comment, parent_id) = {
            let comment = state
                .comments
                .values_mut()
                .flatten()
                .find(|comment| comment.id == comment_id)
                .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
            if comment.status == pb::CommentStatus::Deleted as i32 {
                return Err(RepositoryError::NotFound(comment_id.to_string()));
            }
            if comment.status == pb::CommentStatus::Reviewing as i32 {
                comment.status = status;
            } else if comment.status != status {
                return Err(RepositoryError::ModerationConflict);
            }
            (comment.clone(), comment.parent_id.clone())
        };
        Ok(pb::ReviewCommentResult {
            parent_author_id: memory_parent_author_id(&state, parent_id.as_deref()),
            comment: Some(comment),
        })
    }

    async fn create_report(
        &self,
        input: CreateCommentReportInput,
    ) -> Result<pb::CommentReport, RepositoryError> {
        let mut state = self.state.write().await;
        let request_key = (input.reporter_id.clone(), input.idempotency_key.clone());
        if let Some(report_id) = state.report_requests.get(&request_key)
            && let Some(report) = state.reports.get(report_id)
        {
            if report.reported_comment.as_ref().is_some_and(|comment| {
                comment.id == input.comment_id && comment.post_id == input.post_id
            }) && report.reason == input.reason
                && report.details == input.details
            {
                return Ok(report.clone());
            }
            return Err(RepositoryError::ReportIdempotencyConflict);
        }
        let comment = memory_comment(&state, &input.comment_id)
            .filter(|comment| comment.post_id == input.post_id)
            .ok_or_else(|| RepositoryError::NotFound(input.comment_id.clone()))?;
        if comment.author_id == input.reporter_id {
            return Err(RepositoryError::SelfReport);
        }
        if comment.status != pb::CommentStatus::Published as i32
            || input.excluded_author_ids.contains(&comment.author_id)
        {
            return Err(RepositoryError::NotReportable(input.comment_id));
        }
        let report = pb::CommentReport {
            id: input.id.clone(),
            reporter_id: input.reporter_id.clone(),
            reported_comment: Some(comment),
            reason: input.reason,
            details: input.details,
            status: pb::CommentReportStatus::Pending as i32,
            reviewer_id: None,
            resolution: None,
            action: pb::CommentReportAction::NoAction as i32,
            created_at: input.created_at.clone(),
            updated_at: input.created_at,
        };
        state.report_requests.insert(request_key, report.id.clone());
        state.reports.insert(report.id.clone(), report.clone());
        Ok(report)
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentReport>, RepositoryError> {
        let state = self.state.read().await;
        let mut reports = state
            .reports
            .values()
            .filter(|report| status.is_none_or(|status| report.status == status))
            .filter(|report| {
                cursor.is_none_or(|cursor| {
                    CommentCursor::from_values(&report.created_at, &report.id).is_some_and(
                        |value| {
                            (value.created_at, value.id.as_str())
                                > (cursor.created_at, cursor.id.as_str())
                        },
                    )
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        reports.sort_by_key(|report| CommentCursor::from_values(&report.created_at, &report.id));
        reports.truncate(limit);
        Ok(reports)
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewCommentReportInput,
    ) -> Result<pb::CommentReport, RepositoryError> {
        let mut state = self.state.write().await;
        let mut report = state
            .reports
            .get(report_id)
            .cloned()
            .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))?;
        let was_terminal = is_terminal_report(report.status);
        let mut reviewed = apply_report_review(&mut report, &input)?;
        if was_terminal {
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentReportStatus::Resolved as i32
            && reviewed.action == pb::CommentReportAction::RestrictComment as i32
        {
            let comment_id = reviewed
                .reported_comment
                .as_ref()
                .map(|comment| comment.id.clone())
                .ok_or_else(|| RepositoryError::NotFound(report_id.to_string()))?;
            let comment = memory_comment_mut(&mut state, &comment_id)
                .ok_or_else(|| RepositoryError::NotFound(comment_id.clone()))?;
            if comment.status != pb::CommentStatus::Published as i32 {
                return Err(RepositoryError::ActionConflict);
            }
            comment.status = pb::CommentStatus::Restricted as i32;
            let comment = comment.clone();
            reviewed.reported_comment = Some(comment.clone());
            update_memory_comment_records(&mut state, &comment);
        }
        state
            .reports
            .insert(report_id.to_string(), reviewed.clone());
        Ok(reviewed)
    }

    async fn create_appeal(
        &self,
        input: CreateCommentAppealInput,
    ) -> Result<pb::CommentAppeal, RepositoryError> {
        let mut state = self.state.write().await;
        let request_key = (input.author_id.clone(), input.idempotency_key.clone());
        if let Some(appeal_id) = state.appeal_requests.get(&request_key)
            && let Some(appeal) = state.appeals.get(appeal_id)
        {
            if appeal
                .appealed_comment
                .as_ref()
                .is_some_and(|comment| comment.id == input.comment_id)
                && appeal.details == input.details
            {
                return Ok(appeal.clone());
            }
            return Err(RepositoryError::AppealIdempotencyConflict);
        }
        let comment = memory_comment(&state, &input.comment_id)
            .ok_or_else(|| RepositoryError::NotFound(input.comment_id.clone()))?;
        if comment.author_id != input.author_id
            || comment.status != pb::CommentStatus::Restricted as i32
        {
            return Err(RepositoryError::ActionConflict);
        }
        let appeal = pb::CommentAppeal {
            id: input.id.clone(),
            author_id: input.author_id.clone(),
            appealed_comment: Some(comment),
            details: input.details,
            status: pb::CommentAppealStatus::Pending as i32,
            reviewer_id: None,
            resolution: None,
            action: pb::CommentAppealAction::NoAction as i32,
            created_at: input.created_at.clone(),
            updated_at: input.created_at,
        };
        state.appeal_requests.insert(request_key, appeal.id.clone());
        state.appeals.insert(appeal.id.clone(), appeal.clone());
        Ok(appeal)
    }

    async fn list_appeals(
        &self,
        author_id: Option<&str>,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentAppeal>, RepositoryError> {
        let state = self.state.read().await;
        let mut appeals = state
            .appeals
            .values()
            .filter(|appeal| author_id.is_none_or(|author_id| appeal.author_id == author_id))
            .filter(|appeal| status.is_none_or(|status| appeal.status == status))
            .filter(|appeal| {
                cursor.is_none_or(|cursor| {
                    CommentCursor::from_values(&appeal.created_at, &appeal.id).is_some_and(
                        |value| {
                            (value.created_at, value.id.as_str())
                                > (cursor.created_at, cursor.id.as_str())
                        },
                    )
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        appeals.sort_by_key(|appeal| CommentCursor::from_values(&appeal.created_at, &appeal.id));
        appeals.truncate(limit);
        Ok(appeals)
    }

    async fn review_appeal(
        &self,
        appeal_id: &str,
        input: ReviewCommentAppealInput,
    ) -> Result<pb::CommentAppeal, RepositoryError> {
        let mut state = self.state.write().await;
        let mut appeal = state
            .appeals
            .get(appeal_id)
            .cloned()
            .ok_or_else(|| RepositoryError::AppealNotFound(appeal_id.to_string()))?;
        let was_terminal = is_terminal_appeal(appeal.status);
        let mut reviewed = apply_appeal_review(&mut appeal, &input)?;
        if was_terminal {
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentAppealStatus::Resolved as i32
            && reviewed.action == pb::CommentAppealAction::RestoreComment as i32
        {
            let comment_id = reviewed
                .appealed_comment
                .as_ref()
                .map(|comment| comment.id.clone())
                .ok_or_else(|| RepositoryError::NotFound(appeal_id.to_string()))?;
            let comment = memory_comment_mut(&mut state, &comment_id)
                .ok_or_else(|| RepositoryError::NotFound(comment_id.clone()))?;
            if comment.status != pb::CommentStatus::Restricted as i32 {
                return Err(RepositoryError::ActionConflict);
            }
            comment.status = pb::CommentStatus::Published as i32;
            let comment = comment.clone();
            reviewed.appealed_comment = Some(comment.clone());
            update_memory_comment_records(&mut state, &comment);
        }
        state
            .appeals
            .insert(appeal_id.to_string(), reviewed.clone());
        Ok(reviewed)
    }
}

pub(crate) struct PostgresCommentRepository {
    pool: sqlx::PgPool,
}

impl PostgresCommentRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct CommentReportRow {
    report_id: String,
    reporter_id: String,
    reason: String,
    details: String,
    status: String,
    reviewer_id: Option<String>,
    resolution: Option<String>,
    action: String,
    report_created_at: OffsetDateTime,
    report_updated_at: OffsetDateTime,
    comment_id: String,
    post_id: String,
    author_id: String,
    parent_id: Option<String>,
    body: String,
    like_count: i64,
    comment_created_at: OffsetDateTime,
    moderation_state: String,
    comment_deleted: bool,
}

#[derive(sqlx::FromRow)]
struct CommentAppealRow {
    appeal_id: String,
    appeal_author_id: String,
    details: String,
    status: String,
    reviewer_id: Option<String>,
    resolution: Option<String>,
    action: String,
    appeal_created_at: OffsetDateTime,
    appeal_updated_at: OffsetDateTime,
    comment_id: String,
    post_id: String,
    comment_author_id: String,
    parent_id: Option<String>,
    body: String,
    like_count: i64,
    comment_created_at: OffsetDateTime,
    moderation_state: String,
    comment_deleted: bool,
}

#[async_trait]
impl CommentRepository for PostgresCommentRepository {
    async fn get(
        &self,
        post_id: &str,
        comment_id: &str,
        excluded_author_ids: &[String],
    ) -> Result<pb::CommentItem, RepositoryError> {
        let row = sqlx::query_as::<_, StoredCommentRow>(
            "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted_at IS NOT NULL FROM comments WHERE id = $1 AND post_id = $2 AND deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($3::TEXT[]) = 0 OR author_id <> ALL($3::TEXT[]))",
        )
        .bind(comment_id)
        .bind(post_id)
        .bind(excluded_author_ids)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        comment_from_stored_row(row)
    }

    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<pb::CommentItem>, RepositoryError> {
        let rows = sqlx::query_as::<
            _,
            (
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
            "WITH RECURSIVE visible_published AS (SELECT id, parent_id FROM comments WHERE post_id = $1 AND deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($4::TEXT[]) = 0 OR author_id <> ALL($4::TEXT[]))), visible_deleted_ancestors (id, parent_id) AS (SELECT parent.id, parent.parent_id FROM comments AS parent JOIN visible_published AS child ON child.parent_id = parent.id WHERE parent.post_id = $1 AND parent.deleted_at IS NOT NULL AND (cardinality($4::TEXT[]) = 0 OR parent.author_id <> ALL($4::TEXT[])) UNION SELECT parent.id, parent.parent_id FROM comments AS parent JOIN visible_deleted_ancestors AS descendant ON descendant.parent_id = parent.id WHERE parent.post_id = $1 AND parent.deleted_at IS NOT NULL AND (cardinality($4::TEXT[]) = 0 OR parent.author_id <> ALL($4::TEXT[]))) SELECT id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted_at IS NOT NULL FROM comments WHERE post_id = $1 AND ((deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($4::TEXT[]) = 0 OR author_id <> ALL($4::TEXT[]))) OR (deleted_at IS NOT NULL AND id IN (SELECT id FROM visible_deleted_ancestors))) AND ($2::TIMESTAMPTZ IS NULL OR (created_at, id) > ($2::TIMESTAMPTZ, $3)) ORDER BY created_at ASC, id ASC LIMIT $5",
        )
        .bind(post_id)
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(excluded_author_ids)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(
                |(
                    id,
                    author_id,
                    parent_id,
                    body,
                    like_count,
                    created_at,
                    moderation_state,
                    deleted,
                )| {
                    if deleted {
                        Ok(deleted_comment(
                            id,
                            post_id.to_string(),
                            parent_id,
                            format_timestamp(created_at),
                        ))
                    } else {
                        Ok(pb::CommentItem {
                            id,
                            post_id: post_id.to_string(),
                            author_id: author_id.clone(),
                            author_name: author_id,
                            body,
                            parent_id,
                            like_count: like_count.max(0) as u64,
                            created_at: format_timestamp(created_at),
                            status: moderation_status(&moderation_state)?,
                        })
                    }
                },
            )
            .collect::<Result<Vec<_>, RepositoryError>>()
    }

    async fn list_moderation(
        &self,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentItem>, RepositoryError> {
        let rows = sqlx::query_as::<
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
            ),
        >(
            "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state FROM comments WHERE deleted_at IS NULL AND moderation_state = 'reviewing' AND ($1::TIMESTAMPTZ IS NULL OR (created_at, id) > ($1::TIMESTAMPTZ, $2)) ORDER BY created_at ASC, id ASC LIMIT $3",
        )
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(comment_from_row)
            .collect::<Result<Vec<_>, RepositoryError>>()
    }

    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<pb::CreateCommentResult, RepositoryError> {
        let CreateCommentInput {
            user_id,
            post_id,
            author_name,
            body,
            parent_id,
            excluded_author_ids,
            idempotency_key,
        } = input;
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(key) = idempotency_key.as_deref()
            && let Some(existing) = find_idempotent_comment(&mut tx, user_id, key).await?
        {
            if existing.comment.post_id == post_id
                && existing.request_body == body
                && existing.comment.parent_id == parent_id
            {
                let parent_author_id =
                    find_parent_author_id(&mut tx, existing.comment.parent_id.as_deref()).await?;
                tx.commit().await.map_err(RepositoryError::Database)?;
                return Ok(pb::CreateCommentResult {
                    comment: Some(existing.comment),
                    parent_author_id,
                });
            }
            return Err(RepositoryError::IdempotencyConflict);
        }
        let (parent_author_id, depth) = if let Some(parent) = parent_id.as_deref() {
            let (parent_author_id, parent_depth) = sqlx::query_as::<_, (String, i16)>(
                "SELECT author_id, depth FROM comments WHERE id = $1 AND post_id = $2 AND deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($3::TEXT[]) = 0 OR author_id <> ALL($3::TEXT[]))",
            )
            .bind(parent)
            .bind(post_id)
            .bind(excluded_author_ids)
            .fetch_optional(&mut *tx)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or_else(|| RepositoryError::ParentNotFound(parent.to_string()))?
            ;
            if parent_depth < 0 {
                return Err(RepositoryError::InvalidReplyHierarchy);
            }
            if parent_depth as usize >= MAX_REPLY_DEPTH {
                return Err(RepositoryError::ReplyDepthExceeded);
            }
            (Some(parent_author_id), parent_depth + 1)
        } else {
            (None, 0)
        };
        let id = uuid::Uuid::now_v7().to_string();
        let inserted = sqlx::query_as::<_, (time::OffsetDateTime, String)>(
            "INSERT INTO comments (id, post_id, author_id, parent_id, body, depth, moderation_state, client_request_id) VALUES ($1,$2,$3,$4,$5,$6,'reviewing',$7) ON CONFLICT (author_id, client_request_id) WHERE client_request_id IS NOT NULL DO NOTHING RETURNING created_at, moderation_state",
        )
        .bind(&id).bind(post_id).bind(user_id).bind(&parent_id).bind(&body)
        .bind(depth).bind(&idempotency_key)
        .fetch_optional(&mut *tx).await.map_err(RepositoryError::Database)?;
        let Some((created_at, moderation_state)) = inserted else {
            let existing = find_idempotent_comment(
                &mut tx,
                user_id,
                idempotency_key.as_deref().unwrap_or_default(),
            )
            .await?
            .ok_or(RepositoryError::IdempotencyConflict)?;
            if existing.comment.post_id != post_id
                || existing.request_body != body
                || existing.comment.parent_id != parent_id
            {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let parent_author_id =
                find_parent_author_id(&mut tx, existing.comment.parent_id.as_deref()).await?;
            tx.commit().await.map_err(RepositoryError::Database)?;
            return Ok(pb::CreateCommentResult {
                comment: Some(existing.comment),
                parent_author_id,
            });
        };
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(pb::CreateCommentResult {
            comment: Some(pb::CommentItem {
                id,
                post_id: post_id.to_string(),
                author_id: user_id.to_string(),
                author_name: author_name.to_string(),
                body,
                parent_id,
                like_count: 0,
                created_at: format_timestamp(created_at),
                status: moderation_status(&moderation_state)?,
            }),
            parent_author_id,
        })
    }

    async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query_scalar::<_, String>(
            "UPDATE comments SET deleted_at = COALESCE(deleted_at, now()), updated_at = CASE WHEN deleted_at IS NULL THEN now() ELSE updated_at END WHERE id = $1 AND post_id = $2 AND author_id = $3 RETURNING id",
        )
        .bind(comment_id)
        .bind(post_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        Ok(())
    }

    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: i32,
    ) -> Result<pb::CommentItem, RepositoryError> {
        let state = moderation_state_name(status)
            .ok_or_else(|| RepositoryError::InvalidModerationState(format!("{status:?}")))?;
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
            ),
        >(
            "UPDATE comments SET moderation_state = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'reviewing' RETURNING id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state",
        )
        .bind(comment_id)
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        if let Some(row) = row {
            return comment_from_row(row);
        }
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
            ),
        >(
            "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state FROM comments WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(comment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        comment_from_row(row)
    }

    async fn review(
        &self,
        comment_id: &str,
        reviewer_id: &str,
        status: i32,
    ) -> Result<pb::ReviewCommentResult, RepositoryError> {
        let state = moderation_state_name(status)
            .filter(|state| *state != "reviewing")
            .ok_or_else(|| RepositoryError::InvalidModerationState(format!("{status:?}")))?;
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
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
            ),
        >(
            "UPDATE comments SET moderation_state = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'reviewing' RETURNING id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state",
        )
        .bind(comment_id)
        .bind(state)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?;
        let comment = if let Some(row) = row {
            sqlx::query(
                "INSERT INTO comment_moderation_reviews (comment_id, reviewer_id, decision) VALUES ($1, $2, $3) ON CONFLICT (comment_id) DO NOTHING",
            )
            .bind(comment_id)
            .bind(reviewer_id)
            .bind(state)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::Database)?;
            comment_from_row(row)?
        } else {
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
                ),
            >(
                "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state FROM comments WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(comment_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
            let comment = comment_from_row(row)?;
            if comment.status != status {
                return Err(RepositoryError::ModerationConflict);
            }
            comment
        };
        let parent_author_id = find_parent_author_id(&mut tx, comment.parent_id.as_deref()).await?;
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(pb::ReviewCommentResult {
            comment: Some(comment),
            parent_author_id,
        })
    }

    async fn create_report(
        &self,
        input: CreateCommentReportInput,
    ) -> Result<pb::CommentReport, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(existing) = select_comment_report_by_idempotency(
            &mut transaction,
            &input.reporter_id,
            &input.idempotency_key,
        )
        .await?
        {
            let report = existing.into_report()?;
            let same_request = report.reported_comment.as_ref().is_some_and(|comment| {
                comment.id == input.comment_id && comment.post_id == input.post_id
            }) && report.reason == input.reason
                && report.details == input.details;
            if !same_request {
                return Err(RepositoryError::ReportIdempotencyConflict);
            }
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return Ok(report);
        }

        let comment = select_stored_comment(&mut transaction, &input.comment_id)
            .await?
            .filter(|comment| comment.1 == input.post_id)
            .ok_or_else(|| RepositoryError::NotFound(input.comment_id.clone()))?;
        let reportable = comment.2 != input.reporter_id
            && !comment.8
            && comment.7 == "published"
            && !input.excluded_author_ids.contains(&comment.2);
        if comment.2 == input.reporter_id {
            return Err(RepositoryError::SelfReport);
        }
        if !reportable {
            return Err(RepositoryError::NotReportable(input.comment_id));
        }

        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO comment_reports (id,comment_id,reporter_id,reason,details,idempotency_key,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7::TIMESTAMPTZ,$7::TIMESTAMPTZ) ON CONFLICT (reporter_id,idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING RETURNING id",
        )
        .bind(&input.id)
        .bind(&input.comment_id)
        .bind(&input.reporter_id)
        .bind(comment_report_reason_name(input.reason)?)
        .bind(&input.details)
        .bind(&input.idempotency_key)
        .bind(&input.created_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let report_id = match inserted {
            Some(report_id) => report_id,
            None => {
                let existing = select_comment_report_by_idempotency(
                    &mut transaction,
                    &input.reporter_id,
                    &input.idempotency_key,
                )
                .await?
                .ok_or(RepositoryError::ReportIdempotencyConflict)?
                .into_report()?;
                let same_request = existing.reported_comment.as_ref().is_some_and(|comment| {
                    comment.id == input.comment_id && comment.post_id == input.post_id
                }) && existing.reason == input.reason
                    && existing.details == input.details;
                if !same_request {
                    return Err(RepositoryError::ReportIdempotencyConflict);
                }
                transaction
                    .commit()
                    .await
                    .map_err(RepositoryError::Database)?;
                return Ok(existing);
            }
        };
        let report = select_comment_report(&mut transaction, &report_id)
            .await?
            .into_report()?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(report)
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentReport>, RepositoryError> {
        let rows = sqlx::query_as::<_, CommentReportRow>(
            "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE ($1::TEXT IS NULL OR r.status = $1) AND ($2::TIMESTAMPTZ IS NULL OR (r.created_at,r.id) > ($2,$3)) ORDER BY r.created_at ASC,r.id ASC LIMIT $4",
        )
        .bind(status.map(comment_report_status_name).transpose()?)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(CommentReportRow::into_report)
            .collect()
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewCommentReportInput,
    ) -> Result<pb::CommentReport, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let mut report = select_comment_report_for_update(&mut transaction, report_id)
            .await?
            .into_report()?;
        let was_terminal = is_terminal_report(report.status);
        let reviewed = apply_report_review(&mut report, &input)?;
        if was_terminal {
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentReportStatus::Resolved as i32
            && reviewed.action == pb::CommentReportAction::RestrictComment as i32
        {
            let comment_id = reviewed
                .reported_comment
                .as_ref()
                .map(|comment| comment.id.as_str())
                .ok_or_else(|| RepositoryError::NotFound(report_id.to_string()))?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE comments SET moderation_state = 'restricted',updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'published' RETURNING id",
            )
            .bind(comment_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if changed.is_none() {
                return Err(RepositoryError::ActionConflict);
            }
        }
        sqlx::query(
            "UPDATE comment_reports SET status = $2,reviewer_id = $3,resolution = $4,action = $5,updated_at = now() WHERE id = $1",
        )
        .bind(report_id)
        .bind(comment_report_status_name(reviewed.status)?)
        .bind(&reviewed.reviewer_id)
        .bind(&reviewed.resolution)
        .bind(comment_report_action_name(reviewed.action)?)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let reviewed = select_comment_report(&mut transaction, report_id)
            .await?
            .into_report()?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(reviewed)
    }

    async fn create_appeal(
        &self,
        input: CreateCommentAppealInput,
    ) -> Result<pb::CommentAppeal, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(existing) = select_comment_appeal_by_idempotency(
            &mut transaction,
            &input.author_id,
            &input.idempotency_key,
        )
        .await?
        {
            let appeal = existing.into_appeal()?;
            let same_request = appeal
                .appealed_comment
                .as_ref()
                .is_some_and(|comment| comment.id == input.comment_id)
                && appeal.details == input.details;
            if !same_request {
                return Err(RepositoryError::AppealIdempotencyConflict);
            }
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return Ok(appeal);
        }
        let comment = select_stored_comment(&mut transaction, &input.comment_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound(input.comment_id.clone()))?;
        if comment.2 != input.author_id || comment.8 || comment.7 != "restricted" {
            return Err(RepositoryError::ActionConflict);
        }
        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO comment_appeals (id,comment_id,author_id,details,idempotency_key,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6::TIMESTAMPTZ,$6::TIMESTAMPTZ) ON CONFLICT (author_id,idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING RETURNING id",
        )
        .bind(&input.id)
        .bind(&input.comment_id)
        .bind(&input.author_id)
        .bind(&input.details)
        .bind(&input.idempotency_key)
        .bind(&input.created_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let appeal_id = match inserted {
            Some(appeal_id) => appeal_id,
            None => {
                let existing = select_comment_appeal_by_idempotency(
                    &mut transaction,
                    &input.author_id,
                    &input.idempotency_key,
                )
                .await?
                .ok_or(RepositoryError::AppealIdempotencyConflict)?
                .into_appeal()?;
                let same_request = existing
                    .appealed_comment
                    .as_ref()
                    .is_some_and(|comment| comment.id == input.comment_id)
                    && existing.details == input.details;
                if !same_request {
                    return Err(RepositoryError::AppealIdempotencyConflict);
                }
                transaction
                    .commit()
                    .await
                    .map_err(RepositoryError::Database)?;
                return Ok(existing);
            }
        };
        let appeal = select_comment_appeal(&mut transaction, &appeal_id)
            .await?
            .into_appeal()?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(appeal)
    }

    async fn list_appeals(
        &self,
        author_id: Option<&str>,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentAppeal>, RepositoryError> {
        let rows = sqlx::query_as::<_, CommentAppealRow>(
            "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE ($1::TEXT IS NULL OR a.author_id = $1) AND ($2::TEXT IS NULL OR a.status = $2) AND ($3::TIMESTAMPTZ IS NULL OR (a.created_at,a.id) > ($3,$4)) ORDER BY a.created_at ASC,a.id ASC LIMIT $5",
        )
        .bind(author_id)
        .bind(status.map(comment_appeal_status_name).transpose()?)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(CommentAppealRow::into_appeal)
            .collect()
    }

    async fn review_appeal(
        &self,
        appeal_id: &str,
        input: ReviewCommentAppealInput,
    ) -> Result<pb::CommentAppeal, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let mut appeal = select_comment_appeal_for_update(&mut transaction, appeal_id)
            .await?
            .into_appeal()?;
        let was_terminal = is_terminal_appeal(appeal.status);
        let reviewed = apply_appeal_review(&mut appeal, &input)?;
        if was_terminal {
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentAppealStatus::Resolved as i32
            && reviewed.action == pb::CommentAppealAction::RestoreComment as i32
        {
            let comment_id = reviewed
                .appealed_comment
                .as_ref()
                .map(|comment| comment.id.as_str())
                .ok_or_else(|| RepositoryError::NotFound(appeal_id.to_string()))?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE comments SET moderation_state = 'published',updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'restricted' RETURNING id",
            )
            .bind(comment_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if changed.is_none() {
                return Err(RepositoryError::ActionConflict);
            }
        }
        sqlx::query(
            "UPDATE comment_appeals SET status = $2,reviewer_id = $3,resolution = $4,action = $5,updated_at = now() WHERE id = $1",
        )
        .bind(appeal_id)
        .bind(comment_appeal_status_name(reviewed.status)?)
        .bind(&reviewed.reviewer_id)
        .bind(&reviewed.resolution)
        .bind(comment_appeal_action_name(reviewed.action)?)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let reviewed = select_comment_appeal(&mut transaction, appeal_id)
            .await?
            .into_appeal()?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(reviewed)
    }
}

type StoredCommentRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
    OffsetDateTime,
    String,
    bool,
);

impl CommentReportRow {
    fn into_report(self) -> Result<pb::CommentReport, RepositoryError> {
        Ok(pb::CommentReport {
            id: self.report_id,
            reporter_id: self.reporter_id,
            reported_comment: Some(moderation_comment_from_stored_row((
                self.comment_id,
                self.post_id,
                self.author_id,
                self.parent_id,
                self.body,
                self.like_count,
                self.comment_created_at,
                self.moderation_state,
                self.comment_deleted,
            ))?),
            reason: parse_comment_report_reason(&self.reason)?,
            details: self.details,
            status: parse_comment_report_status(&self.status)?,
            reviewer_id: self.reviewer_id,
            resolution: self.resolution,
            action: parse_comment_report_action(&self.action)?,
            created_at: format_timestamp(self.report_created_at),
            updated_at: format_timestamp(self.report_updated_at),
        })
    }
}

impl CommentAppealRow {
    fn into_appeal(self) -> Result<pb::CommentAppeal, RepositoryError> {
        Ok(pb::CommentAppeal {
            id: self.appeal_id,
            author_id: self.appeal_author_id,
            appealed_comment: Some(moderation_comment_from_stored_row((
                self.comment_id,
                self.post_id,
                self.comment_author_id,
                self.parent_id,
                self.body,
                self.like_count,
                self.comment_created_at,
                self.moderation_state,
                self.comment_deleted,
            ))?),
            details: self.details,
            status: parse_comment_appeal_status(&self.status)?,
            reviewer_id: self.reviewer_id,
            resolution: self.resolution,
            action: parse_comment_appeal_action(&self.action)?,
            created_at: format_timestamp(self.appeal_created_at),
            updated_at: format_timestamp(self.appeal_updated_at),
        })
    }
}

async fn select_stored_comment(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    comment_id: &str,
) -> Result<Option<StoredCommentRow>, RepositoryError> {
    sqlx::query_as::<_, StoredCommentRow>(
        "SELECT id,post_id,author_id,parent_id,body,like_count,created_at,moderation_state,deleted_at IS NOT NULL FROM comments WHERE id = $1 FOR UPDATE",
    )
    .bind(comment_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)
}

async fn select_comment_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<CommentReportRow, RepositoryError> {
    sqlx::query_as::<_, CommentReportRow>(
        "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE r.id = $1",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))
}

async fn select_comment_report_by_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    reporter_id: &str,
    idempotency_key: &str,
) -> Result<Option<CommentReportRow>, RepositoryError> {
    sqlx::query_as::<_, CommentReportRow>(
        "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE r.reporter_id = $1 AND r.idempotency_key = $2",
    )
    .bind(reporter_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)
}

async fn select_comment_report_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<CommentReportRow, RepositoryError> {
    sqlx::query_as::<_, CommentReportRow>(
        "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE r.id = $1 FOR UPDATE OF r,c",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))
}

async fn select_comment_appeal(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    appeal_id: &str,
) -> Result<CommentAppealRow, RepositoryError> {
    sqlx::query_as::<_, CommentAppealRow>(
        "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE a.id = $1",
    )
    .bind(appeal_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::AppealNotFound(appeal_id.to_string()))
}

async fn select_comment_appeal_by_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    author_id: &str,
    idempotency_key: &str,
) -> Result<Option<CommentAppealRow>, RepositoryError> {
    sqlx::query_as::<_, CommentAppealRow>(
        "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE a.author_id = $1 AND a.idempotency_key = $2",
    )
    .bind(author_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)
}

async fn select_comment_appeal_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    appeal_id: &str,
) -> Result<CommentAppealRow, RepositoryError> {
    sqlx::query_as::<_, CommentAppealRow>(
        "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE a.id = $1 FOR UPDATE OF a,c",
    )
    .bind(appeal_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::AppealNotFound(appeal_id.to_string()))
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
) -> Result<pb::CommentReport, RepositoryError> {
    let status = pb::CommentReportStatus::try_from(input.status).map_err(|_| {
        RepositoryError::InvalidModerationState("unknown comment report status".to_string())
    })?;
    let action = pb::CommentReportAction::try_from(input.action).map_err(|_| {
        RepositoryError::InvalidModerationState("unknown comment report action".to_string())
    })?;
    if is_terminal_report(report.status) {
        return (report.status == input.status
            && report.resolution.as_deref() == Some(input.resolution.as_str())
            && report.action == input.action)
            .then(|| report.clone())
            .ok_or(RepositoryError::ReportConflict);
    }
    if status == pb::CommentReportStatus::Pending {
        return Err(RepositoryError::InvalidModerationState(
            "pending is not a review decision".to_string(),
        ));
    }
    if status == pb::CommentReportStatus::Reviewing
        && (!input.resolution.is_empty() || action != pb::CommentReportAction::NoAction)
    {
        return Err(RepositoryError::InvalidModerationState(
            "reviewing reports cannot resolve or restrict a comment".to_string(),
        ));
    }
    if is_terminal_report(input.status) && input.resolution.is_empty() {
        return Err(RepositoryError::InvalidModerationState(
            "terminal reviews require a resolution".to_string(),
        ));
    }
    if status == pb::CommentReportStatus::Rejected && action != pb::CommentReportAction::NoAction {
        return Err(RepositoryError::InvalidModerationState(
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
) -> Result<pb::CommentAppeal, RepositoryError> {
    let status = pb::CommentAppealStatus::try_from(input.status).map_err(|_| {
        RepositoryError::InvalidModerationState("unknown comment appeal status".to_string())
    })?;
    let action = pb::CommentAppealAction::try_from(input.action).map_err(|_| {
        RepositoryError::InvalidModerationState("unknown comment appeal action".to_string())
    })?;
    if is_terminal_appeal(appeal.status) {
        return (appeal.status == input.status
            && appeal.resolution.as_deref() == Some(input.resolution.as_str())
            && appeal.action == input.action)
            .then(|| appeal.clone())
            .ok_or(RepositoryError::AppealConflict);
    }
    if status == pb::CommentAppealStatus::Pending {
        return Err(RepositoryError::InvalidModerationState(
            "pending is not a review decision".to_string(),
        ));
    }
    if status == pb::CommentAppealStatus::Reviewing
        && (!input.resolution.is_empty() || action != pb::CommentAppealAction::NoAction)
    {
        return Err(RepositoryError::InvalidModerationState(
            "reviewing appeals cannot resolve or restore a comment".to_string(),
        ));
    }
    if is_terminal_appeal(input.status) && input.resolution.is_empty() {
        return Err(RepositoryError::InvalidModerationState(
            "terminal appeal reviews require a resolution".to_string(),
        ));
    }
    if status == pb::CommentAppealStatus::Rejected && action != pb::CommentAppealAction::NoAction {
        return Err(RepositoryError::InvalidModerationState(
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
) -> Result<usize, RepositoryError> {
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
            .ok_or_else(|| RepositoryError::ParentNotFound(parent_id.to_string()))?;
    }
    Ok(depth)
}

async fn find_parent_author_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    parent_id: Option<&str>,
) -> Result<Option<String>, RepositoryError> {
    let Some(parent_id) = parent_id else {
        return Ok(None);
    };
    sqlx::query_scalar::<_, String>("SELECT author_id FROM comments WHERE id = $1")
        .bind(parent_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(RepositoryError::Database)
}

async fn find_idempotent_comment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    key: &str,
) -> Result<Option<IdempotentComment>, RepositoryError> {
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
    .map_err(RepositoryError::Database)?;
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
) -> Result<pb::CommentItem, RepositoryError> {
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
) -> Result<pb::CommentItem, RepositoryError> {
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
) -> Result<pb::CommentItem, RepositoryError> {
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

fn moderation_status(value: &str) -> Result<i32, RepositoryError> {
    match value {
        "reviewing" => Ok(pb::CommentStatus::Reviewing as i32),
        "published" => Ok(pb::CommentStatus::Published as i32),
        "restricted" => Ok(pb::CommentStatus::Restricted as i32),
        value => Err(RepositoryError::InvalidModerationState(value.to_string())),
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

fn comment_report_reason_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::CommentReportReason::try_from(value) {
        Ok(pb::CommentReportReason::Spam) => Ok("spam"),
        Ok(pb::CommentReportReason::Harassment) => Ok("harassment"),
        Ok(pb::CommentReportReason::Unsafe) => Ok("unsafe"),
        Ok(pb::CommentReportReason::Fraud) => Ok("fraud"),
        Ok(pb::CommentReportReason::Privacy) => Ok("privacy"),
        Ok(pb::CommentReportReason::Other) => Ok("other"),
        Ok(pb::CommentReportReason::Unspecified) | Err(_) => Err(
            RepositoryError::InvalidModerationState("unknown comment report reason".to_string()),
        ),
    }
}

fn parse_comment_report_reason(value: &str) -> Result<i32, RepositoryError> {
    let reason = match value {
        "spam" => pb::CommentReportReason::Spam,
        "harassment" => pb::CommentReportReason::Harassment,
        "unsafe" => pb::CommentReportReason::Unsafe,
        "fraud" => pb::CommentReportReason::Fraud,
        "privacy" => pb::CommentReportReason::Privacy,
        "other" => pb::CommentReportReason::Other,
        _ => {
            return Err(RepositoryError::InvalidModerationState(format!(
                "unknown comment report reason {value}"
            )));
        }
    };
    Ok(reason as i32)
}

fn comment_report_status_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::CommentReportStatus::try_from(value) {
        Ok(pb::CommentReportStatus::Pending) => Ok("pending"),
        Ok(pb::CommentReportStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::CommentReportStatus::Resolved) => Ok("resolved"),
        Ok(pb::CommentReportStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(RepositoryError::InvalidModerationState(format!(
            "unknown comment report status {value}"
        ))),
    }
}

fn parse_comment_report_status(value: &str) -> Result<i32, RepositoryError> {
    let status = match value {
        "pending" => pb::CommentReportStatus::Pending,
        "reviewing" => pb::CommentReportStatus::Reviewing,
        "resolved" => pb::CommentReportStatus::Resolved,
        "rejected" => pb::CommentReportStatus::Rejected,
        _ => {
            return Err(RepositoryError::InvalidModerationState(format!(
                "unknown comment report status {value}"
            )));
        }
    };
    Ok(status as i32)
}

fn comment_report_action_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::CommentReportAction::try_from(value) {
        Ok(pb::CommentReportAction::NoAction) => Ok("no_action"),
        Ok(pb::CommentReportAction::RestrictComment) => Ok("restrict_comment"),
        Err(_) => Err(RepositoryError::InvalidModerationState(format!(
            "unknown comment report action {value}"
        ))),
    }
}

fn parse_comment_report_action(value: &str) -> Result<i32, RepositoryError> {
    let action = match value {
        "no_action" => pb::CommentReportAction::NoAction,
        "restrict_comment" => pb::CommentReportAction::RestrictComment,
        _ => {
            return Err(RepositoryError::InvalidModerationState(format!(
                "unknown comment report action {value}"
            )));
        }
    };
    Ok(action as i32)
}

fn comment_appeal_status_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::CommentAppealStatus::try_from(value) {
        Ok(pb::CommentAppealStatus::Pending) => Ok("pending"),
        Ok(pb::CommentAppealStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::CommentAppealStatus::Resolved) => Ok("resolved"),
        Ok(pb::CommentAppealStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(RepositoryError::InvalidModerationState(format!(
            "unknown comment appeal status {value}"
        ))),
    }
}

fn parse_comment_appeal_status(value: &str) -> Result<i32, RepositoryError> {
    let status = match value {
        "pending" => pb::CommentAppealStatus::Pending,
        "reviewing" => pb::CommentAppealStatus::Reviewing,
        "resolved" => pb::CommentAppealStatus::Resolved,
        "rejected" => pb::CommentAppealStatus::Rejected,
        _ => {
            return Err(RepositoryError::InvalidModerationState(format!(
                "unknown comment appeal status {value}"
            )));
        }
    };
    Ok(status as i32)
}

fn comment_appeal_action_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::CommentAppealAction::try_from(value) {
        Ok(pb::CommentAppealAction::NoAction) => Ok("no_action"),
        Ok(pb::CommentAppealAction::RestoreComment) => Ok("restore_comment"),
        Err(_) => Err(RepositoryError::InvalidModerationState(format!(
            "unknown comment appeal action {value}"
        ))),
    }
}

fn parse_comment_appeal_action(value: &str) -> Result<i32, RepositoryError> {
    let action = match value {
        "no_action" => pb::CommentAppealAction::NoAction,
        "restore_comment" => pb::CommentAppealAction::RestoreComment,
        _ => {
            return Err(RepositoryError::InvalidModerationState(format!(
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

    async fn published_comment(repository: &MemoryCommentRepository) -> pb::CommentItem {
        let comment = repository
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
        repository
            .set_moderation_status(&comment.id, pb::CommentStatus::Published as i32)
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
        let repository = MemoryCommentRepository::default();
        let comment = published_comment(&repository).await;
        let report = repository
            .create_report(report_input(&comment.id, "repeated harassment"))
            .await
            .expect("report published comment");
        let retry = repository
            .create_report(report_input(&comment.id, "repeated harassment"))
            .await
            .expect("idempotent report retry");
        let conflicting_retry = repository
            .create_report(report_input(&comment.id, "different details"))
            .await;
        let own_report = repository
            .create_report(CreateCommentReportInput {
                reporter_id: "author".to_string(),
                idempotency_key: "report-own".to_string(),
                ..report_input(&comment.id, "self report")
            })
            .await;
        let hidden_report = repository
            .create_report(CreateCommentReportInput {
                idempotency_key: "report-hidden".to_string(),
                excluded_author_ids: vec!["author".to_string()],
                ..report_input(&comment.id, "hidden report")
            })
            .await;

        assert_eq!(report.id, retry.id);
        assert!(matches!(
            conflicting_retry,
            Err(RepositoryError::ReportIdempotencyConflict)
        ));
        assert!(matches!(own_report, Err(RepositoryError::SelfReport)));
        assert!(matches!(
            hidden_report,
            Err(RepositoryError::NotReportable(_))
        ));

        let restricted = repository
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
            repository
                .list("post", None, 10, &[])
                .await
                .expect("public comments after restriction")
                .is_empty()
        );

        let appeal = repository
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
        let restored = repository
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
        let conflicting_terminal_review = repository
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
            repository
                .list("post", None, 10, &[])
                .await
                .expect("public comments after restore")
                .len(),
            1
        );
        assert!(matches!(
            conflicting_terminal_review,
            Err(RepositoryError::ReportConflict)
        ));
    }

    #[tokio::test]
    async fn get_returns_only_a_visible_published_comment_on_the_requested_post() {
        let repository = MemoryCommentRepository::default();
        let comment = published_comment(&repository).await;

        let fetched = repository
            .get("post", &comment.id, &[])
            .await
            .expect("published answer is readable");
        assert_eq!(fetched.id, comment.id);
        assert!(matches!(
            repository
                .get("post", &comment.id, &["author".to_string()])
                .await,
            Err(RepositoryError::NotFound(_))
        ));
        assert!(matches!(
            repository.get("other-post", &comment.id, &[]).await,
            Err(RepositoryError::NotFound(_))
        ));
    }
}
