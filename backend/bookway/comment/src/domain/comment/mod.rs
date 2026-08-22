use std::collections::BTreeSet;

use bookway_content_audit_api::pb as audit_pb;

use crate::{
    api::pb,
    datasource::{
        CommentCursor, CreateCommentAppealInput, CreateCommentInput, CreateCommentReportInput,
        ReviewCommentAppealInput, ReviewCommentReportInput,
    },
    domain::{CommentError, Domain},
};

const DEFAULT_PAGE_SIZE: usize = 30;
const MAX_PAGE_SIZE: usize = 50;
const DEFAULT_MODERATION_PAGE_SIZE: usize = 50;
const MAX_MODERATION_PAGE_SIZE: usize = 100;
const MAX_REVIEWER_ID_LENGTH: usize = 256;
const MAX_ID_LENGTH: usize = 160;
const MAX_REPORT_DETAILS_LENGTH: usize = 1_000;
const MAX_REVIEW_RESOLUTION_LENGTH: usize = 1_000;

impl Domain {
    pub(crate) async fn get(
        &self,
        request: pb::GetRequest,
    ) -> Result<pb::CommentItem, CommentError> {
        if request.post_id.trim().is_empty() || request.comment_id.trim().is_empty() {
            return Err(CommentError::Validation(
                "内容和评论 ID 不能为空".to_string(),
            ));
        }
        Ok(self
            .dao
            .get(
                request.post_id.trim(),
                request.comment_id.trim(),
                &normalized_excluded_author_ids(request.excluded_author_ids),
            )
            .await?)
    }

    pub(crate) async fn list(
        &self,
        request: pb::ListRequest,
    ) -> Result<pb::CommentPage, CommentError> {
        if request.post_id.trim().is_empty() {
            return Err(CommentError::Validation("内容 ID 不能为空".to_string()));
        }
        let cursor = request
            .cursor
            .as_deref()
            .map(|value| {
                CommentCursor::decode(value)
                    .ok_or_else(|| CommentError::Validation("评论游标无效".to_string()))
            })
            .transpose()?;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE as u32)
            .clamp(1, MAX_PAGE_SIZE as u32) as usize;
        let excluded_author_ids = normalized_excluded_author_ids(request.excluded_author_ids);
        let mut items = self
            .dao
            .list(
                &request.post_id,
                cursor.as_ref(),
                limit + 1,
                &excluded_author_ids,
            )
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(CommentCursor::from_comment))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::CommentPage { items, next_cursor })
    }

    pub(crate) async fn create(
        &self,
        request: pb::CreateRequest,
    ) -> Result<pb::CreateCommentResult, CommentError> {
        if request.user_id.trim().is_empty() || request.post_id.trim().is_empty() {
            return Err(CommentError::Validation(
                "用户和内容 ID 不能为空".to_string(),
            ));
        }
        if request.body.trim().is_empty() {
            return Err(CommentError::Validation("评论不能为空".to_string()));
        }
        if request.body.chars().count() > 1000 {
            return Err(CommentError::Validation(
                "评论不能超过 1000 个字符".to_string(),
            ));
        }
        let idempotency_key = request
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if idempotency_key.as_ref().is_some_and(|key| {
            key.len() > 128
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
        }) {
            return Err(CommentError::Validation("评论幂等键格式无效".to_string()));
        }
        let excluded_author_ids = normalized_excluded_author_ids(request.excluded_author_ids);
        let mut result = self
            .dao
            .create(CreateCommentInput {
                user_id: &request.user_id,
                post_id: &request.post_id,
                author_name: &request.user_id,
                body: request.body.trim().to_string(),
                parent_id: request.parent_id,
                excluded_author_ids: &excluded_author_ids,
                idempotency_key,
            })
            .await?;
        let comment = result.comment.as_mut().ok_or_else(|| {
            CommentError::Repository(crate::datasource::DaoError::InvalidReplyHierarchy)
        })?;
        if comment.status != pb::CommentStatus::Reviewing as i32 {
            return Ok(result);
        }
        let status = match self
            .audit(audit_pb::AuditRequest {
                content_id: comment.id.clone(),
                version: 1,
                title: "评论".to_string(),
                body: comment.body.clone(),
            })
            .await
        {
            Ok(response) => moderation_status(response.decision),
            Err(error) => {
                // The database default is reviewing, so audit outages fail closed.
                tracing::warn!(comment_id = %comment.id, %error, "comment audit degraded");
                return Ok(result);
            }
        };
        if comment.status == status {
            return Ok(result);
        }
        let comment_id = comment.id.clone();
        result.comment = Some(self.dao.set_moderation_status(&comment_id, status).await?);
        Ok(result)
    }

    pub(crate) async fn list_moderation(
        &self,
        request: pb::ListModerationRequest,
    ) -> Result<pb::ModerationCommentPage, CommentError> {
        let cursor = request
            .cursor
            .as_deref()
            .map(|value| {
                CommentCursor::decode(value)
                    .ok_or_else(|| CommentError::Validation("评论审核游标无效".to_string()))
            })
            .transpose()?;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_MODERATION_PAGE_SIZE as u32)
            .clamp(1, MAX_MODERATION_PAGE_SIZE as u32) as usize;
        let mut items = self.dao.list_moderation(cursor.as_ref(), limit + 1).await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(CommentCursor::from_comment))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::ModerationCommentPage { items, next_cursor })
    }

    pub(crate) async fn review(
        &self,
        request: pb::ReviewCommentRequest,
    ) -> Result<pb::ReviewCommentResult, CommentError> {
        let reviewer_id = request.reviewer_id.trim();
        if reviewer_id.is_empty() || reviewer_id.chars().count() > MAX_REVIEWER_ID_LENGTH {
            return Err(CommentError::Validation("审核人身份无效".to_string()));
        }
        let comment_id = request.comment_id.trim();
        if comment_id.is_empty() || comment_id.chars().count() > 160 {
            return Err(CommentError::Validation("评论 ID 无效".to_string()));
        }
        let status = match pb::CommentModerationDecision::try_from(request.decision) {
            Ok(pb::CommentModerationDecision::Approve) => pb::CommentStatus::Published as i32,
            Ok(pb::CommentModerationDecision::Restrict) => pb::CommentStatus::Restricted as i32,
            Ok(pb::CommentModerationDecision::Unspecified) | Err(_) => {
                return Err(CommentError::Validation("必须提供审核决定".to_string()));
            }
        };
        Ok(self.dao.review(comment_id, reviewer_id, status).await?)
    }

    pub(crate) async fn report(
        &self,
        request: pb::CreateCommentReportRequest,
    ) -> Result<pb::CommentReport, CommentError> {
        validate_user_id(&request.reporter_id, "举报人")?;
        validate_id(&request.post_id, "内容")?;
        validate_id(&request.comment_id, "评论")?;
        let reason = comment_report_reason(request.reason)?;
        let details = request.details.trim().to_string();
        if details.chars().count() > MAX_REPORT_DETAILS_LENGTH {
            return Err(CommentError::Validation(
                "举报说明不能超过 1000 个字符".to_string(),
            ));
        }
        let idempotency_key = required_idempotency_key(
            request.idempotency_key,
            "Idempotency-Key 是举报评论所必需的有效标识",
        )?;
        self.dao
            .create_report(CreateCommentReportInput {
                id: uuid::Uuid::now_v7().to_string(),
                reporter_id: request.reporter_id.trim().to_string(),
                post_id: request.post_id.trim().to_string(),
                comment_id: request.comment_id.trim().to_string(),
                idempotency_key,
                reason: reason as i32,
                details,
                excluded_author_ids: normalized_excluded_author_ids(request.excluded_author_ids),
                created_at: timestamp(),
            })
            .await
            .map_err(CommentError::from)
    }

    pub(crate) async fn list_reports(
        &self,
        request: pb::ListCommentReportsRequest,
    ) -> Result<pb::CommentReportPage, CommentError> {
        let status = request.status.map(comment_report_status).transpose()?;
        let cursor = decode_cursor(request.cursor, "评论举报游标无效")?;
        let limit = moderation_page_size(request.limit);
        let mut items = self
            .dao
            .list_reports(status.map(|value| value as i32), cursor.as_ref(), limit + 1)
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| {
                items
                    .last()
                    .and_then(|report| CommentCursor::from_values(&report.created_at, &report.id))
            })
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::CommentReportPage { items, next_cursor })
    }

    pub(crate) async fn review_report(
        &self,
        mut request: pb::ReviewCommentReportRequest,
    ) -> Result<pb::CommentReport, CommentError> {
        validate_user_id(&request.reviewer_id, "审核人")?;
        validate_id(&request.report_id, "举报")?;
        let status = comment_report_status(request.status)?;
        let action = comment_report_action(request.action)?;
        request.resolution = request.resolution.trim().to_string();
        if request.resolution.chars().count() > MAX_REVIEW_RESOLUTION_LENGTH {
            return Err(CommentError::Validation(
                "审核说明不能超过 1000 个字符".to_string(),
            ));
        }
        validate_report_review_transition(status, action, &request.resolution)?;
        self.dao
            .review_report(
                request.report_id.trim(),
                ReviewCommentReportInput {
                    reviewer_id: request.reviewer_id.trim().to_string(),
                    status: status as i32,
                    resolution: request.resolution,
                    action: action as i32,
                },
            )
            .await
            .map_err(CommentError::from)
    }

    pub(crate) async fn appeal(
        &self,
        request: pb::CreateCommentAppealRequest,
    ) -> Result<pb::CommentAppeal, CommentError> {
        validate_user_id(&request.author_id, "评论作者")?;
        validate_id(&request.comment_id, "评论")?;
        let details = request.details.trim().to_string();
        if details.chars().count() > MAX_REPORT_DETAILS_LENGTH {
            return Err(CommentError::Validation(
                "申诉说明不能超过 1000 个字符".to_string(),
            ));
        }
        let idempotency_key = required_idempotency_key(
            request.idempotency_key,
            "Idempotency-Key 是评论申诉所必需的有效标识",
        )?;
        self.dao
            .create_appeal(CreateCommentAppealInput {
                id: uuid::Uuid::now_v7().to_string(),
                author_id: request.author_id.trim().to_string(),
                comment_id: request.comment_id.trim().to_string(),
                idempotency_key,
                details,
                created_at: timestamp(),
            })
            .await
            .map_err(CommentError::from)
    }

    pub(crate) async fn list_appeals(
        &self,
        request: pb::ListCommentAppealsRequest,
    ) -> Result<pb::CommentAppealPage, CommentError> {
        let author_id = request
            .author_id
            .as_deref()
            .map(|author_id| {
                validate_user_id(author_id, "评论作者")?;
                Ok::<&str, CommentError>(author_id.trim())
            })
            .transpose()?;
        let status = request.status.map(comment_appeal_status).transpose()?;
        let cursor = decode_cursor(request.cursor, "评论申诉游标无效")?;
        let limit = moderation_page_size(request.limit);
        let mut items = self
            .dao
            .list_appeals(
                author_id,
                status.map(|value| value as i32),
                cursor.as_ref(),
                limit + 1,
            )
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| {
                items
                    .last()
                    .and_then(|appeal| CommentCursor::from_values(&appeal.created_at, &appeal.id))
            })
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::CommentAppealPage { items, next_cursor })
    }

    pub(crate) async fn review_appeal(
        &self,
        mut request: pb::ReviewCommentAppealRequest,
    ) -> Result<pb::CommentAppeal, CommentError> {
        validate_user_id(&request.reviewer_id, "审核人")?;
        validate_id(&request.appeal_id, "申诉")?;
        let status = comment_appeal_status(request.status)?;
        let action = comment_appeal_action(request.action)?;
        request.resolution = request.resolution.trim().to_string();
        if request.resolution.chars().count() > MAX_REVIEW_RESOLUTION_LENGTH {
            return Err(CommentError::Validation(
                "审核说明不能超过 1000 个字符".to_string(),
            ));
        }
        validate_appeal_review_transition(status, action, &request.resolution)?;
        self.dao
            .review_appeal(
                request.appeal_id.trim(),
                ReviewCommentAppealInput {
                    reviewer_id: request.reviewer_id.trim().to_string(),
                    status: status as i32,
                    resolution: request.resolution,
                    action: action as i32,
                },
            )
            .await
            .map_err(CommentError::from)
    }

    pub(crate) async fn delete(&self, request: pb::DeleteRequest) -> Result<(), CommentError> {
        if request.comment_id.trim().is_empty() {
            return Err(CommentError::Validation("评论 ID 不能为空".to_string()));
        }
        self.dao
            .delete(&request.user_id, &request.post_id, &request.comment_id)
            .await?;
        Ok(())
    }
}

fn validate_user_id(value: &str, label: &str) -> Result<(), CommentError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_REVIEWER_ID_LENGTH {
        return Err(CommentError::Validation(format!("{label}身份无效")));
    }
    Ok(())
}

fn validate_id(value: &str, label: &str) -> Result<(), CommentError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_ID_LENGTH {
        return Err(CommentError::Validation(format!("{label} ID 无效")));
    }
    Ok(())
}

fn required_idempotency_key(value: Option<String>, message: &str) -> Result<String, CommentError> {
    let value = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CommentError::Validation(message.to_string()))?;
    if value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
    {
        return Err(CommentError::Validation(message.to_string()));
    }
    Ok(value.to_string())
}

fn decode_cursor(
    cursor: Option<String>,
    message: &str,
) -> Result<Option<CommentCursor>, CommentError> {
    cursor
        .as_deref()
        .map(|value| {
            CommentCursor::decode(value)
                .ok_or_else(|| CommentError::Validation(message.to_string()))
        })
        .transpose()
}

fn moderation_page_size(value: Option<u32>) -> usize {
    value
        .unwrap_or(DEFAULT_MODERATION_PAGE_SIZE as u32)
        .clamp(1, MAX_MODERATION_PAGE_SIZE as u32) as usize
}

fn comment_report_reason(value: i32) -> Result<pb::CommentReportReason, CommentError> {
    match pb::CommentReportReason::try_from(value) {
        Ok(pb::CommentReportReason::Spam) => Ok(pb::CommentReportReason::Spam),
        Ok(pb::CommentReportReason::Harassment) => Ok(pb::CommentReportReason::Harassment),
        Ok(pb::CommentReportReason::Unsafe) => Ok(pb::CommentReportReason::Unsafe),
        Ok(pb::CommentReportReason::Fraud) => Ok(pb::CommentReportReason::Fraud),
        Ok(pb::CommentReportReason::Privacy) => Ok(pb::CommentReportReason::Privacy),
        Ok(pb::CommentReportReason::Other) => Ok(pb::CommentReportReason::Other),
        Ok(pb::CommentReportReason::Unspecified) | Err(_) => {
            Err(CommentError::Validation("举报原因无效".to_string()))
        }
    }
}

fn comment_report_status(value: i32) -> Result<pb::CommentReportStatus, CommentError> {
    pb::CommentReportStatus::try_from(value)
        .map_err(|_| CommentError::Validation("评论举报状态无效".to_string()))
}

fn comment_report_action(value: i32) -> Result<pb::CommentReportAction, CommentError> {
    pb::CommentReportAction::try_from(value)
        .map_err(|_| CommentError::Validation("评论举报审核动作无效".to_string()))
}

fn comment_appeal_status(value: i32) -> Result<pb::CommentAppealStatus, CommentError> {
    pb::CommentAppealStatus::try_from(value)
        .map_err(|_| CommentError::Validation("评论申诉状态无效".to_string()))
}

fn comment_appeal_action(value: i32) -> Result<pb::CommentAppealAction, CommentError> {
    pb::CommentAppealAction::try_from(value)
        .map_err(|_| CommentError::Validation("评论申诉审核动作无效".to_string()))
}

fn validate_report_review_transition(
    status: pb::CommentReportStatus,
    action: pb::CommentReportAction,
    resolution: &str,
) -> Result<(), CommentError> {
    match status {
        pb::CommentReportStatus::Pending => Err(CommentError::Validation(
            "pending 不是人工审核决定".to_string(),
        )),
        pb::CommentReportStatus::Reviewing
            if !resolution.is_empty() || action != pb::CommentReportAction::NoAction =>
        {
            Err(CommentError::Validation(
                "reviewing 举报不能设置结论或限制评论".to_string(),
            ))
        }
        pb::CommentReportStatus::Resolved | pb::CommentReportStatus::Rejected
            if resolution.is_empty() =>
        {
            Err(CommentError::Validation(
                "终态审核决定必须包含说明".to_string(),
            ))
        }
        pb::CommentReportStatus::Rejected if action != pb::CommentReportAction::NoAction => {
            Err(CommentError::Validation("驳回举报不能限制评论".to_string()))
        }
        _ => Ok(()),
    }
}

fn validate_appeal_review_transition(
    status: pb::CommentAppealStatus,
    action: pb::CommentAppealAction,
    resolution: &str,
) -> Result<(), CommentError> {
    match status {
        pb::CommentAppealStatus::Pending => Err(CommentError::Validation(
            "pending 不是人工审核决定".to_string(),
        )),
        pb::CommentAppealStatus::Reviewing
            if !resolution.is_empty() || action != pb::CommentAppealAction::NoAction =>
        {
            Err(CommentError::Validation(
                "reviewing 申诉不能设置结论或恢复评论".to_string(),
            ))
        }
        pb::CommentAppealStatus::Resolved | pb::CommentAppealStatus::Rejected
            if resolution.is_empty() =>
        {
            Err(CommentError::Validation(
                "终态审核决定必须包含说明".to_string(),
            ))
        }
        pb::CommentAppealStatus::Rejected if action != pb::CommentAppealAction::NoAction => {
            Err(CommentError::Validation("驳回申诉不能恢复评论".to_string()))
        }
        _ => Ok(()),
    }
}

fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn normalized_excluded_author_ids(author_ids: Vec<String>) -> Vec<String> {
    author_ids
        .into_iter()
        .map(|author_id| author_id.trim().to_string())
        .filter(|author_id| !author_id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn moderation_status(decision: i32) -> i32 {
    match audit_pb::AuditDecision::try_from(decision) {
        Ok(audit_pb::AuditDecision::Approved) => pb::CommentStatus::Published as i32,
        Ok(audit_pb::AuditDecision::Reviewing) => pb::CommentStatus::Reviewing as i32,
        Ok(audit_pb::AuditDecision::Restricted) | Err(_) => pb::CommentStatus::Restricted as i32,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{conf::Config, datasource::MemoryCommentDao};

    fn domain() -> Domain {
        Domain {
            config: Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
                content_audit_grpc_url: None,
            },
            dao: Arc::new(MemoryCommentDao::default()),
            content_audit: None,
        }
    }

    async fn pending_comment(domain: &Domain, comment_idempotency_key: &str) -> pb::CommentItem {
        domain
            .dao
            .create(CreateCommentInput {
                user_id: "reader-a",
                post_id: "post-a",
                author_name: "reader-a",
                body: "请人工审核这条评论".to_string(),
                parent_id: None,
                excluded_author_ids: &[],
                idempotency_key: Some(comment_idempotency_key.to_string()),
            })
            .await
            .expect("create reviewing comment")
            .comment
            .expect("comment result")
    }

    #[tokio::test]
    async fn manual_approval_completes_a_reviewing_comment_idempotently() {
        let domain = domain();
        let pending = pending_comment(&domain, "comment-review-1").await;

        let queue = domain
            .list_moderation(pb::ListModerationRequest {
                cursor: None,
                limit: Some(20),
            })
            .await
            .expect("moderation queue");
        assert_eq!(queue.items, vec![pending.clone()]);

        let request = pb::ReviewCommentRequest {
            reviewer_id: "moderator-a".to_string(),
            comment_id: pending.id.clone(),
            decision: pb::CommentModerationDecision::Approve as i32,
        };
        let approved = domain
            .review(request.clone())
            .await
            .expect("approve comment")
            .comment
            .expect("approved comment");
        let retry = domain
            .review(request)
            .await
            .expect("idempotent approval retry")
            .comment
            .expect("approved comment");

        assert_eq!(approved.status, pb::CommentStatus::Published as i32);
        assert_eq!(approved, retry);
        assert!(
            domain
                .list_moderation(pb::ListModerationRequest {
                    cursor: None,
                    limit: None,
                })
                .await
                .expect("empty moderation queue")
                .items
                .is_empty()
        );
    }

    #[tokio::test]
    async fn manual_decision_cannot_be_overwritten_by_another_reviewer_or_auto_audit() {
        let domain = domain();
        let pending = pending_comment(&domain, "comment-review-2").await;
        let restricted = domain
            .review(pb::ReviewCommentRequest {
                reviewer_id: "moderator-a".to_string(),
                comment_id: pending.id.clone(),
                decision: pb::CommentModerationDecision::Restrict as i32,
            })
            .await
            .expect("restrict comment")
            .comment
            .expect("restricted comment");

        let conflicting = domain
            .review(pb::ReviewCommentRequest {
                reviewer_id: "moderator-b".to_string(),
                comment_id: pending.id.clone(),
                decision: pb::CommentModerationDecision::Approve as i32,
            })
            .await;
        assert!(matches!(
            conflicting,
            Err(CommentError::Repository(
                crate::datasource::DaoError::ModerationConflict
            ))
        ));

        let automatic = domain
            .dao
            .set_moderation_status(&pending.id, pb::CommentStatus::Published as i32)
            .await
            .expect("late automatic audit leaves manual decision intact");
        assert_eq!(restricted.status, pb::CommentStatus::Restricted as i32);
        assert_eq!(automatic.status, pb::CommentStatus::Restricted as i32);
    }

    #[test]
    fn report_and_appeal_decisions_require_safe_terminal_transitions() {
        assert!(
            validate_report_review_transition(
                pb::CommentReportStatus::Reviewing,
                pb::CommentReportAction::NoAction,
                "",
            )
            .is_ok()
        );
        assert!(
            validate_report_review_transition(
                pb::CommentReportStatus::Resolved,
                pb::CommentReportAction::RestrictComment,
                "restricted after review",
            )
            .is_ok()
        );
        assert!(
            validate_report_review_transition(
                pb::CommentReportStatus::Rejected,
                pb::CommentReportAction::RestrictComment,
                "cannot apply this action",
            )
            .is_err()
        );
        assert!(
            validate_appeal_review_transition(
                pb::CommentAppealStatus::Resolved,
                pb::CommentAppealAction::RestoreComment,
                "restored after review",
            )
            .is_ok()
        );
        assert!(
            validate_appeal_review_transition(
                pb::CommentAppealStatus::Pending,
                pb::CommentAppealAction::NoAction,
                "",
            )
            .is_err()
        );
        assert!(
            validate_appeal_review_transition(
                pb::CommentAppealStatus::Rejected,
                pb::CommentAppealAction::RestoreComment,
                "cannot restore",
            )
            .is_err()
        );
    }
}
