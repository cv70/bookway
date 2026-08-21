use bookway_content_audit_api::pb as audit_pb;

use crate::{
    api::pb,
    datasource::{
        ConversationCursor, CreateMessageReportInput, MessageCursor, ReportCursor, DaoError,
        ReviewMessageReportInput, SendMessageInput,
    },
    domain::{Domain, MessageError},
};

const DEFAULT_CONVERSATION_PAGE_SIZE: usize = 30;
const DEFAULT_MESSAGE_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 100;
const MAX_USER_ID_LENGTH: usize = 160;
const MAX_BODY_LENGTH: usize = 2_000;
const MAX_REPORT_DETAILS_LENGTH: usize = 1_000;
const MAX_REPORT_RESOLUTION_LENGTH: usize = 1_000;

impl Domain {
    pub(crate) async fn send(
        &self,
        request: pb::SendDirectMessageRequest,
    ) -> Result<pb::DirectMessage, MessageError> {
        validate_user_id(&request.sender_user_id)?;
        validate_user_id(&request.recipient_user_id)?;
        let sender_user_id = request.sender_user_id.trim().to_string();
        let recipient_user_id = request.recipient_user_id.trim().to_string();
        if sender_user_id == recipient_user_id {
            return Err(MessageError::Validation("不能向自己发送私信".to_string()));
        }
        validate_client_message_id(&request.client_message_id)?;
        let kind = pb::DirectMessageKind::try_from(request.kind)
            .map_err(|_| MessageError::Validation("私信类型无效".to_string()))?;
        if kind != pb::DirectMessageKind::Text {
            return Err(MessageError::Validation("暂只支持文本私信".to_string()));
        }
        let body = request.body.trim();
        if body.is_empty() || body.chars().count() > MAX_BODY_LENGTH {
            return Err(MessageError::Validation("私信正文无效".to_string()));
        }
        if self.Dao.sender_restricted(&sender_user_id).await? {
            return Err(MessageError::SenderRestricted);
        }

        // Blocking is enforced from BBS's source-of-truth in both directions.
        let sender_context = self.social_context(sender_user_id.clone()).await?;
        let recipient_context = self.social_context(recipient_user_id.clone()).await?;
        if sender_context
            .blocked_author_ids
            .contains(&recipient_user_id)
            || recipient_context
                .blocked_author_ids
                .contains(&sender_user_id)
        {
            return Err(MessageError::Blocked);
        }
        if !self
            .Dao
            .preferences(&recipient_user_id)
            .await?
            .allow_direct_messages
        {
            return Err(MessageError::RecipientUnavailable);
        }
        let audit = self
            .audit_message(
                format!(
                    "direct-message:{sender_user_id}:{recipient_user_id}:{}",
                    request.client_message_id
                ),
                body.to_string(),
            )
            .await?;
        allow_audited_message(audit.decision)?;
        self.Dao
            .send(SendMessageInput {
                sender_user_id,
                recipient_user_id,
                client_message_id: request.client_message_id,
                kind: kind as i32,
                body: body.to_string(),
            })
            .await
            .map_err(|error| match error {
                DaoError::SenderRestricted => MessageError::SenderRestricted,
                error => MessageError::Dao(error),
            })
    }

    pub(crate) async fn list_conversations(
        &self,
        request: pb::ListConversationsRequest,
    ) -> Result<pb::ConversationPage, MessageError> {
        validate_user_id(&request.user_id)?;
        let user_id = request.user_id.trim();
        let cursor = match request.cursor.as_deref() {
            Some(value) => Some(
                ConversationCursor::decode(value)
                    .ok_or_else(|| MessageError::Validation("私信会话游标无效".to_string()))?,
            ),
            None => None,
        };
        let limit = page_size(request.limit, DEFAULT_CONVERSATION_PAGE_SIZE);
        let mut items = self
            .Dao
            .list_conversations(user_id, cursor.as_ref(), limit + 1)
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(ConversationCursor::from_conversation))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::ConversationPage { items, next_cursor })
    }

    pub(crate) async fn list_messages(
        &self,
        request: pb::ListMessagesRequest,
    ) -> Result<pb::DirectMessagePage, MessageError> {
        validate_user_id(&request.user_id)?;
        validate_conversation_id(&request.conversation_id)?;
        let user_id = request.user_id.trim();
        let conversation_id = request.conversation_id.trim();
        let cursor = match request.cursor.as_deref() {
            Some(value) => Some(
                MessageCursor::decode(value)
                    .ok_or_else(|| MessageError::Validation("私信消息游标无效".to_string()))?,
            ),
            None => None,
        };
        let limit = page_size(request.limit, DEFAULT_MESSAGE_PAGE_SIZE);
        let mut items = self
            .Dao
            .list_messages(user_id, conversation_id, cursor.as_ref(), limit + 1)
            .await?;
        let has_more = items.len() > limit;
        if has_more {
            // The Dao fetches newest first then restores chronological
            // order. The extra item is the oldest and belongs to the next page.
            items.remove(0);
        }
        let next_cursor = has_more
            .then(|| items.first().and_then(MessageCursor::from_message))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::DirectMessagePage { items, next_cursor })
    }

    pub(crate) async fn mark_conversation_read(
        &self,
        request: pb::MarkConversationReadRequest,
    ) -> Result<pb::MarkConversationReadResponse, MessageError> {
        validate_user_id(&request.user_id)?;
        validate_conversation_id(&request.conversation_id)?;
        let user_id = request.user_id.trim();
        let conversation_id = request.conversation_id.trim();
        if request
            .through_message_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 160)
        {
            return Err(MessageError::Validation("消息 ID 无效".to_string()));
        }
        Ok(self
            .Dao
            .mark_read(
                user_id,
                conversation_id,
                request.through_message_id.as_deref().map(str::trim),
            )
            .await?)
    }

    pub(crate) async fn get_preferences(
        &self,
        request: pb::UserRequest,
    ) -> Result<pb::DirectMessagePreferences, MessageError> {
        validate_user_id(&request.user_id)?;
        Ok(self.Dao.preferences(request.user_id.trim()).await?)
    }

    pub(crate) async fn update_preferences(
        &self,
        request: pb::UpdateDirectMessagePreferencesRequest,
    ) -> Result<pb::DirectMessagePreferences, MessageError> {
        validate_user_id(&request.user_id)?;
        Ok(self
            .Dao
            .update_preferences(request.user_id.trim(), request.allow_direct_messages)
            .await?)
    }

    pub(crate) async fn report(
        &self,
        request: pb::ReportDirectMessageRequest,
    ) -> Result<pb::DirectMessageReport, MessageError> {
        validate_user_id(&request.reporter_user_id)?;
        validate_message_id(&request.message_id)?;
        let reason = report_reason(request.reason)?;
        let details = request.details.trim().to_string();
        if details.chars().count() > MAX_REPORT_DETAILS_LENGTH {
            return Err(MessageError::Validation(
                "举报说明不能超过 1000 个字符".to_string(),
            ));
        }
        let idempotency_key = normalize_report_idempotency_key(request.idempotency_key)?;
        self.Dao
            .create_report(CreateMessageReportInput {
                id: uuid::Uuid::now_v7().to_string(),
                reporter_user_id: request.reporter_user_id.trim().to_string(),
                message_id: request.message_id.trim().to_string(),
                idempotency_key,
                reason: reason as i32,
                details,
                created_at: timestamp(),
            })
            .await
            .map_err(MessageError::from)
    }

    pub(crate) async fn list_reports(
        &self,
        request: pb::ListDirectMessageReportsRequest,
    ) -> Result<pb::DirectMessageReportPage, MessageError> {
        let status = request.status.map(report_status).transpose()?;
        let cursor = match request.cursor.as_deref() {
            Some(value) => Some(
                ReportCursor::decode(value)
                    .ok_or_else(|| MessageError::Validation("私信举报游标无效".to_string()))?,
            ),
            None => None,
        };
        let limit = page_size(request.limit, DEFAULT_MESSAGE_PAGE_SIZE);
        let mut items = self
            .Dao
            .list_reports(status.map(|value| value as i32), cursor.as_ref(), limit + 1)
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(ReportCursor::from_report))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::DirectMessageReportPage { items, next_cursor })
    }

    pub(crate) async fn review_report(
        &self,
        mut request: pb::ReviewDirectMessageReportRequest,
    ) -> Result<pb::DirectMessageReport, MessageError> {
        validate_user_id(&request.reviewer_user_id)?;
        validate_report_id(&request.report_id)?;
        let status = report_status(request.status)?;
        let action = moderation_action(request.action)?;
        request.resolution = request.resolution.trim().to_string();
        if request.resolution.chars().count() > MAX_REPORT_RESOLUTION_LENGTH {
            return Err(MessageError::Validation(
                "审核说明不能超过 1000 个字符".to_string(),
            ));
        }
        validate_review_transition(status, action, &request.resolution)?;
        self.Dao
            .review_report(
                request.report_id.trim(),
                ReviewMessageReportInput {
                    reviewer_user_id: request.reviewer_user_id.trim().to_string(),
                    status: status as i32,
                    resolution: request.resolution,
                    action: action as i32,
                },
            )
            .await
            .map_err(MessageError::from)
    }
}

fn validate_user_id(value: &str) -> Result<(), MessageError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_USER_ID_LENGTH {
        return Err(MessageError::Validation("用户 ID 无效".to_string()));
    }
    Ok(())
}

fn validate_conversation_id(value: &str) -> Result<(), MessageError> {
    if value.trim().is_empty() || value.chars().count() > 160 {
        return Err(MessageError::Validation("会话 ID 无效".to_string()));
    }
    Ok(())
}

fn validate_message_id(value: &str) -> Result<(), MessageError> {
    if value.trim().is_empty() || value.chars().count() > 160 {
        return Err(MessageError::Validation("消息 ID 无效".to_string()));
    }
    Ok(())
}

fn validate_report_id(value: &str) -> Result<(), MessageError> {
    if value.trim().is_empty() || value.chars().count() > 160 {
        return Err(MessageError::Validation("举报 ID 无效".to_string()));
    }
    Ok(())
}

fn validate_client_message_id(value: &str) -> Result<(), MessageError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
    {
        return Err(MessageError::Validation(
            "Idempotency-Key 是发送私信所必需的有效标识".to_string(),
        ));
    }
    Ok(())
}

fn normalize_report_idempotency_key(value: Option<String>) -> Result<Option<String>, MessageError> {
    let Some(value) = value else {
        return Err(MessageError::Validation(
            "Idempotency-Key 是举报私信所必需的有效标识".to_string(),
        ));
    };
    validate_client_message_id(&value)?;
    Ok(Some(value.trim().to_string()))
}

fn page_size(value: Option<u32>, default: usize) -> usize {
    value
        .unwrap_or(default as u32)
        .clamp(1, MAX_PAGE_SIZE as u32) as usize
}

fn allow_audited_message(decision: i32) -> Result<(), MessageError> {
    match audit_pb::AuditDecision::try_from(decision) {
        Ok(audit_pb::AuditDecision::Approved) => Ok(()),
        Ok(audit_pb::AuditDecision::Reviewing) => Err(MessageError::UnderReview),
        Ok(audit_pb::AuditDecision::Restricted) => Err(MessageError::Restricted),
        Err(_) => Err(MessageError::Audit(
            "audit returned an unknown decision".to_string(),
        )),
    }
}

fn report_reason(value: i32) -> Result<pb::DirectMessageReportReason, MessageError> {
    pb::DirectMessageReportReason::try_from(value)
        .map_err(|_| MessageError::Validation("举报原因无效".to_string()))
}

fn report_status(value: i32) -> Result<pb::DirectMessageReportStatus, MessageError> {
    pb::DirectMessageReportStatus::try_from(value)
        .map_err(|_| MessageError::Validation("举报状态无效".to_string()))
}

fn moderation_action(value: i32) -> Result<pb::DirectMessageModerationAction, MessageError> {
    pb::DirectMessageModerationAction::try_from(value)
        .map_err(|_| MessageError::Validation("审核动作无效".to_string()))
}

fn validate_review_transition(
    status: pb::DirectMessageReportStatus,
    action: pb::DirectMessageModerationAction,
    resolution: &str,
) -> Result<(), MessageError> {
    match status {
        pb::DirectMessageReportStatus::Pending => Err(MessageError::Validation(
            "pending 不是人工审核决定".to_string(),
        )),
        pb::DirectMessageReportStatus::Reviewing
            if !resolution.is_empty() || action != pb::DirectMessageModerationAction::NoAction =>
        {
            Err(MessageError::Validation(
                "reviewing 举报不能设置结论或限制发送者".to_string(),
            ))
        }
        pb::DirectMessageReportStatus::Resolved | pb::DirectMessageReportStatus::Rejected
            if resolution.is_empty() =>
        {
            Err(MessageError::Validation(
                "终态审核决定必须包含说明".to_string(),
            ))
        }
        pb::DirectMessageReportStatus::Rejected
            if action != pb::DirectMessageModerationAction::NoAction =>
        {
            Err(MessageError::Validation(
                "驳回举报不能限制发送者".to_string(),
            ))
        }
        _ => Ok(()),
    }
}

fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_id_rejects_unstable_values() {
        assert!(validate_client_message_id("message-42").is_ok());
        assert!(validate_client_message_id(" ").is_err());
        assert!(validate_client_message_id("contains space").is_err());
    }

    #[test]
    fn page_sizes_are_bounded() {
        assert_eq!(page_size(None, 30), 30);
        assert_eq!(page_size(Some(0), 30), 1);
        assert_eq!(page_size(Some(999), 30), MAX_PAGE_SIZE);
    }

    #[test]
    fn safety_decisions_fail_closed_before_message_persistence() {
        assert!(allow_audited_message(audit_pb::AuditDecision::Approved as i32).is_ok());
        assert!(matches!(
            allow_audited_message(audit_pb::AuditDecision::Reviewing as i32),
            Err(MessageError::UnderReview)
        ));
        assert!(matches!(
            allow_audited_message(audit_pb::AuditDecision::Restricted as i32),
            Err(MessageError::Restricted)
        ));
        assert!(allow_audited_message(99).is_err());
    }

    #[test]
    fn review_transitions_require_a_final_explanation_and_safe_actions() {
        let no_action = pb::DirectMessageModerationAction::NoAction;
        let restrict = pb::DirectMessageModerationAction::RestrictSender;

        assert!(
            validate_review_transition(pb::DirectMessageReportStatus::Reviewing, no_action, "")
                .is_ok()
        );
        assert!(
            validate_review_transition(
                pb::DirectMessageReportStatus::Resolved,
                restrict,
                "restricted after review"
            )
            .is_ok()
        );
        assert!(
            validate_review_transition(pb::DirectMessageReportStatus::Pending, no_action, "")
                .is_err()
        );
        assert!(
            validate_review_transition(
                pb::DirectMessageReportStatus::Rejected,
                restrict,
                "not permitted"
            )
            .is_err()
        );
        assert!(
            validate_review_transition(pb::DirectMessageReportStatus::Resolved, no_action, "")
                .is_err()
        );
    }
}
