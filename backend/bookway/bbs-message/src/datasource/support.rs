use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("conversation {0} was not found")]
    NotFound(String),
    #[error("the current user is not a conversation participant")]
    NotParticipant,
    #[error("a client message id cannot be reused for a different message")]
    IdempotencyConflict,
    #[error("message {0} was not found")]
    MessageNotFound(String),
    #[error("only the recipient of a message can report it")]
    NotMessageRecipient,
    #[error("a report idempotency key cannot be reused for a different report")]
    ReportIdempotencyConflict,
    #[error("report {0} was not found")]
    ReportNotFound(String),
    #[error("report is already in a terminal state")]
    ReportConflict,
    #[error("the sender is restricted from sending direct messages")]
    SenderRestricted,
    #[error("stored message has an invalid kind: {0}")]
    InvalidKind(String),
    #[error("stored report has an invalid {field}: {value}")]
    InvalidReportValue { field: &'static str, value: String },
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConversationCursor {
    pub(crate) last_message_at: OffsetDateTime,
    pub(crate) id: String,
}

impl ConversationCursor {
    pub(crate) fn decode(value: &str) -> Option<Self> {
        let (last_message_at, id) = value.split_once('|')?;
        let last_message_at = OffsetDateTime::parse(last_message_at, &Rfc3339).ok()?;
        (!id.is_empty()).then(|| Self {
            last_message_at,
            id: id.to_string(),
        })
    }

    pub(crate) fn from_conversation(conversation: &pb::Conversation) -> Option<Self> {
        Some(Self {
            last_message_at: OffsetDateTime::parse(&conversation.last_message_at, &Rfc3339).ok()?,
            id: conversation.id.clone(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.last_message_at), self.id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MessageCursor {
    pub(crate) created_at: OffsetDateTime,
    pub(crate) id: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReportCursor {
    pub(crate) created_at: OffsetDateTime,
    pub(crate) id: String,
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

    pub(crate) fn from_report(report: &pb::DirectMessageReport) -> Option<Self> {
        Some(Self {
            created_at: OffsetDateTime::parse(&report.created_at, &Rfc3339).ok()?,
            id: report.id.clone(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.created_at), self.id)
    }
}

impl MessageCursor {
    pub(crate) fn decode(value: &str) -> Option<Self> {
        let (created_at, id) = value.split_once('|')?;
        let created_at = OffsetDateTime::parse(created_at, &Rfc3339).ok()?;
        (!id.is_empty()).then(|| Self {
            created_at,
            id: id.to_string(),
        })
    }

    pub(crate) fn from_message(message: &pb::DirectMessage) -> Option<Self> {
        Some(Self {
            created_at: OffsetDateTime::parse(&message.created_at, &Rfc3339).ok()?,
            id: message.id.clone(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.created_at), self.id)
    }
}

pub(crate) struct SendMessageInput {
    pub(crate) sender_user_id: String,
    pub(crate) recipient_user_id: String,
    pub(crate) client_message_id: String,
    pub(crate) kind: i32,
    pub(crate) body: String,
}

pub(crate) struct CreateMessageReportInput {
    pub(crate) id: String,
    pub(crate) reporter_user_id: String,
    pub(crate) message_id: String,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) reason: i32,
    pub(crate) details: String,
    pub(crate) created_at: String,
}

pub(crate) struct ReviewMessageReportInput {
    pub(crate) reviewer_user_id: String,
    pub(crate) status: i32,
    pub(crate) resolution: String,
    pub(crate) action: i32,
}

#[async_trait]
pub(crate) trait MessageDao: Send + Sync {
    async fn preferences(&self, user_id: &str) -> Result<pb::DirectMessagePreferences, DaoError>;
    async fn update_preferences(
        &self,
        user_id: &str,
        allow_direct_messages: bool,
    ) -> Result<pb::DirectMessagePreferences, DaoError>;
    async fn send(&self, input: SendMessageInput) -> Result<pb::DirectMessage, DaoError>;
    async fn sender_restricted(&self, user_id: &str) -> Result<bool, DaoError>;
    async fn list_conversations(
        &self,
        user_id: &str,
        cursor: Option<&ConversationCursor>,
        limit: usize,
    ) -> Result<Vec<pb::Conversation>, DaoError>;
    async fn list_messages(
        &self,
        user_id: &str,
        conversation_id: &str,
        cursor: Option<&MessageCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessage>, DaoError>;
    async fn mark_read(
        &self,
        user_id: &str,
        conversation_id: &str,
        through_message_id: Option<&str>,
    ) -> Result<pb::MarkConversationReadResponse, DaoError>;
    async fn create_report(
        &self,
        input: CreateMessageReportInput,
    ) -> Result<pb::DirectMessageReport, DaoError>;
    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessageReport>, DaoError>;
    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewMessageReportInput,
    ) -> Result<pb::DirectMessageReport, DaoError>;
}

#[derive(Default)]
struct MemoryState {
    preferences: HashMap<String, pb::DirectMessagePreferences>,
    conversations: HashMap<String, MemoryConversation>,
    messages: Vec<pb::DirectMessage>,
    client_messages: HashMap<(String, String), String>,
    notification_message_ids: HashSet<String>,
    reports: HashMap<String, pb::DirectMessageReport>,
    report_idempotency: HashMap<(String, String), String>,
    restrictions: HashMap<String, String>,
}

#[derive(Clone)]
struct MemoryConversation {
    id: String,
    participant_one_id: String,
    participant_two_id: String,
    last_message_id: String,
    last_message_at: String,
}

fn default_preferences(user_id: &str) -> pb::DirectMessagePreferences {
    pb::DirectMessagePreferences {
        user_id: user_id.to_string(),
        allow_direct_messages: true,
        updated_at: "1970-01-01T00:00:00Z".to_string(),
    }
}

fn sorted_participants(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn conversation_id(left: &str, right: &str) -> String {
    let (left, right) = sorted_participants(left, right);
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("bookway:direct-message:{left}:{right}").as_bytes(),
    )
    .to_string()
}

fn conversation_has_user(conversation: &MemoryConversation, user_id: &str) -> bool {
    conversation.participant_one_id == user_id || conversation.participant_two_id == user_id
}

fn peer_user_id(conversation: &MemoryConversation, user_id: &str) -> String {
    if conversation.participant_one_id == user_id {
        conversation.participant_two_id.clone()
    } else {
        conversation.participant_one_id.clone()
    }
}

fn kind_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::DirectMessageKind::try_from(value) {
        Ok(pb::DirectMessageKind::Text) => Ok("text"),
        Err(_) => Err(DaoError::InvalidKind(value.to_string())),
    }
}

fn parse_kind(value: &str) -> Result<i32, DaoError> {
    match value {
        "text" => Ok(pb::DirectMessageKind::Text as i32),
        value => Err(DaoError::InvalidKind(value.to_string())),
    }
}

fn report_reason_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::DirectMessageReportReason::try_from(value) {
        Ok(pb::DirectMessageReportReason::Spam) => Ok("spam"),
        Ok(pb::DirectMessageReportReason::Harassment) => Ok("harassment"),
        Ok(pb::DirectMessageReportReason::Unsafe) => Ok("unsafe"),
        Ok(pb::DirectMessageReportReason::Fraud) => Ok("fraud"),
        Ok(pb::DirectMessageReportReason::Privacy) => Ok("privacy"),
        Ok(pb::DirectMessageReportReason::Other) => Ok("other"),
        Err(_) => Err(DaoError::InvalidReportValue {
            field: "reason",
            value: value.to_string(),
        }),
    }
}

fn parse_report_reason(value: &str) -> Result<i32, DaoError> {
    let reason = match value {
        "spam" => pb::DirectMessageReportReason::Spam,
        "harassment" => pb::DirectMessageReportReason::Harassment,
        "unsafe" => pb::DirectMessageReportReason::Unsafe,
        "fraud" => pb::DirectMessageReportReason::Fraud,
        "privacy" => pb::DirectMessageReportReason::Privacy,
        "other" => pb::DirectMessageReportReason::Other,
        value => {
            return Err(DaoError::InvalidReportValue {
                field: "reason",
                value: value.to_string(),
            });
        }
    };
    Ok(reason as i32)
}

fn report_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::DirectMessageReportStatus::try_from(value) {
        Ok(pb::DirectMessageReportStatus::Pending) => Ok("pending"),
        Ok(pb::DirectMessageReportStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::DirectMessageReportStatus::Resolved) => Ok("resolved"),
        Ok(pb::DirectMessageReportStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(DaoError::InvalidReportValue {
            field: "status",
            value: value.to_string(),
        }),
    }
}

fn parse_report_status(value: &str) -> Result<i32, DaoError> {
    let status = match value {
        "pending" => pb::DirectMessageReportStatus::Pending,
        "reviewing" => pb::DirectMessageReportStatus::Reviewing,
        "resolved" => pb::DirectMessageReportStatus::Resolved,
        "rejected" => pb::DirectMessageReportStatus::Rejected,
        value => {
            return Err(DaoError::InvalidReportValue {
                field: "status",
                value: value.to_string(),
            });
        }
    };
    Ok(status as i32)
}

fn moderation_action_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::DirectMessageModerationAction::try_from(value) {
        Ok(pb::DirectMessageModerationAction::NoAction) => Ok("no_action"),
        Ok(pb::DirectMessageModerationAction::RestrictSender) => Ok("restrict_sender"),
        Err(_) => Err(DaoError::InvalidReportValue {
            field: "action",
            value: value.to_string(),
        }),
    }
}

fn parse_moderation_action(value: &str) -> Result<i32, DaoError> {
    let action = match value {
        "no_action" => pb::DirectMessageModerationAction::NoAction,
        "restrict_sender" => pb::DirectMessageModerationAction::RestrictSender,
        value => {
            return Err(DaoError::InvalidReportValue {
                field: "action",
                value: value.to_string(),
            });
        }
    };
    Ok(action as i32)
}

fn is_terminal_report(status: i32) -> bool {
    matches!(
        pb::DirectMessageReportStatus::try_from(status),
        Ok(pb::DirectMessageReportStatus::Resolved | pb::DirectMessageReportStatus::Rejected)
    )
}

fn apply_report_review(
    report: &mut pb::DirectMessageReport,
    input: &ReviewMessageReportInput,
) -> Result<pb::DirectMessageReport, DaoError> {
    let status = pb::DirectMessageReportStatus::try_from(input.status).map_err(|_| {
        DaoError::InvalidReportValue {
            field: "status",
            value: input.status.to_string(),
        }
    })?;
    let action = pb::DirectMessageModerationAction::try_from(input.action).map_err(|_| {
        DaoError::InvalidReportValue {
            field: "action",
            value: input.action.to_string(),
        }
    })?;
    if is_terminal_report(report.status) {
        return (report.status == input.status
            && report.resolution.as_deref() == Some(input.resolution.as_str())
            && report.action == input.action)
            .then(|| report.clone())
            .ok_or(DaoError::ReportConflict);
    }
    if status == pb::DirectMessageReportStatus::Pending {
        return Err(DaoError::InvalidReportValue {
            field: "status",
            value: "pending is not a review decision".to_string(),
        });
    }
    if status == pb::DirectMessageReportStatus::Reviewing
        && (!input.resolution.is_empty() || action != pb::DirectMessageModerationAction::NoAction)
    {
        return Err(DaoError::InvalidReportValue {
            field: "review",
            value: "reviewing reports cannot resolve or restrict a sender".to_string(),
        });
    }
    if is_terminal_report(input.status) && input.resolution.is_empty() {
        return Err(DaoError::InvalidReportValue {
            field: "resolution",
            value: "terminal reviews require a resolution".to_string(),
        });
    }
    if status == pb::DirectMessageReportStatus::Rejected
        && action != pb::DirectMessageModerationAction::NoAction
    {
        return Err(DaoError::InvalidReportValue {
            field: "action",
            value: "rejected reports cannot restrict a sender".to_string(),
        });
    }
    report.status = input.status;
    report.reviewer_user_id = Some(input.reviewer_user_id.clone());
    report.resolution = is_terminal_report(input.status).then(|| input.resolution.clone());
    report.action = input.action;
    report.updated_at = now_timestamp();
    Ok(report.clone())
}

fn preview(value: &str) -> String {
    value.chars().take(120).collect()
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

    fn input(
        sender: &str,
        recipient: &str,
        client_message_id: &str,
        body: &str,
    ) -> SendMessageInput {
        SendMessageInput {
            sender_user_id: sender.to_string(),
            recipient_user_id: recipient.to_string(),
            client_message_id: client_message_id.to_string(),
            kind: pb::DirectMessageKind::Text as i32,
            body: body.to_string(),
        }
    }

    fn report_input(
        reporter_user_id: &str,
        message_id: &str,
        idempotency_key: &str,
        details: &str,
    ) -> CreateMessageReportInput {
        CreateMessageReportInput {
            id: Uuid::now_v7().to_string(),
            reporter_user_id: reporter_user_id.to_string(),
            message_id: message_id.to_string(),
            idempotency_key: Some(idempotency_key.to_string()),
            reason: pb::DirectMessageReportReason::Harassment as i32,
            details: details.to_string(),
            created_at: now_timestamp(),
        }
    }

    #[tokio::test]
    async fn retries_return_the_same_message_and_conflicts_are_rejected() {
        let Dao = MemoryMessageDao::default();
        let first = Dao
            .send(input("reader-a", "creator-b", "client-1", "你好"))
            .await
            .expect("initial send");
        let retry = Dao
            .send(input("reader-a", "creator-b", "client-1", "你好"))
            .await
            .expect("retry");
        let conflict = Dao
            .send(input("reader-a", "creator-b", "client-1", "另一条正文"))
            .await;

        assert_eq!(first.id, retry.id);
        assert!(matches!(conflict, Err(DaoError::IdempotencyConflict)));
        let state = Dao.state.read().await;
        assert_eq!(state.notification_message_ids.len(), 1);
        assert!(state.notification_message_ids.contains(&first.id));
    }

    #[tokio::test]
    async fn reading_marks_only_recipient_messages_and_conversation_is_visible_to_both_sides() {
        let Dao = MemoryMessageDao::default();
        let first = Dao
            .send(input("reader-a", "creator-b", "client-1", "第一条"))
            .await
            .expect("first message");
        Dao.send(input("creator-b", "reader-a", "client-2", "收到"))
            .await
            .expect("reply");

        let creator_page = Dao
            .list_conversations("creator-b", None, 10)
            .await
            .expect("creator conversations");
        assert_eq!(creator_page.len(), 1);
        assert_eq!(creator_page[0].peer_user_id, "reader-a");
        assert_eq!(creator_page[0].unread_count, 1);

        let read = Dao
            .mark_read("creator-b", &first.conversation_id, Some(&first.id))
            .await
            .expect("mark first message read");
        assert_eq!(read.marked_count, 1);
        let reader_page = Dao
            .list_conversations("reader-a", None, 10)
            .await
            .expect("reader conversations");
        assert_eq!(reader_page[0].unread_count, 1);
        let creator_page = Dao
            .list_conversations("creator-b", None, 10)
            .await
            .expect("creator conversations after read");
        assert_eq!(creator_page[0].unread_count, 0);
    }

    #[tokio::test]
    async fn message_pages_are_chronological_and_continue_from_the_oldest_item() {
        let Dao = MemoryMessageDao::default();
        let mut messages = Vec::new();
        for index in 0..4 {
            messages.push(
                Dao.send(input(
                    "reader-a",
                    "creator-b",
                    &format!("client-{index}"),
                    &format!("message-{index}"),
                ))
                .await
                .expect("send message"),
            );
        }
        let page = Dao
            .list_messages("reader-a", &messages[0].conversation_id, None, 2)
            .await
            .expect("first page");
        assert_eq!(page.len(), 2);
        assert!(page[0].created_at <= page[1].created_at);
        let cursor = MessageCursor::from_message(&page[0]).expect("cursor");
        let older = Dao
            .list_messages("reader-a", &messages[0].conversation_id, Some(&cursor), 2)
            .await
            .expect("older page");
        assert!(older.iter().all(|message| message.id != page[0].id));
    }

    #[tokio::test]
    async fn only_the_recipient_can_report_and_retries_are_idempotent() {
        let Dao = MemoryMessageDao::default();
        let message = Dao
            .send(input(
                "sender",
                "recipient",
                "message-1",
                "unwanted message",
            ))
            .await
            .expect("send message");

        let report = Dao
            .create_report(report_input(
                "recipient",
                &message.id,
                "report-1",
                "repeated abuse",
            ))
            .await
            .expect("recipient report");
        let retry = Dao
            .create_report(report_input(
                "recipient",
                &message.id,
                "report-1",
                "repeated abuse",
            ))
            .await
            .expect("report retry");
        let sender_attempt = Dao
            .create_report(report_input(
                "sender",
                &message.id,
                "report-2",
                "not allowed",
            ))
            .await;
        let conflicting_retry = Dao
            .create_report(report_input(
                "recipient",
                &message.id,
                "report-1",
                "different details",
            ))
            .await;

        assert_eq!(report.id, retry.id);
        assert_eq!(report.reported_user_id, "sender");
        assert_eq!(
            report
                .reported_message
                .as_ref()
                .expect("reported message")
                .body,
            "unwanted message"
        );
        assert!(matches!(sender_attempt, Err(DaoError::NotMessageRecipient)));
        assert!(matches!(
            conflicting_retry,
            Err(DaoError::ReportIdempotencyConflict)
        ));
    }

    #[tokio::test]
    async fn resolved_restrictions_block_future_messages_and_terminal_reviews_conflict() {
        let Dao = MemoryMessageDao::default();
        let message = Dao
            .send(input("sender", "recipient", "message-1", "unsafe message"))
            .await
            .expect("send message");
        let report = Dao
            .create_report(report_input("recipient", &message.id, "report-1", "unsafe"))
            .await
            .expect("report message");
        let reviewed = Dao
            .review_report(
                &report.id,
                ReviewMessageReportInput {
                    reviewer_user_id: "moderator".to_string(),
                    status: pb::DirectMessageReportStatus::Resolved as i32,
                    resolution: "sender restricted for safety".to_string(),
                    action: pb::DirectMessageModerationAction::RestrictSender as i32,
                },
            )
            .await
            .expect("restrict sender");
        let conflicting_review = Dao
            .review_report(
                &report.id,
                ReviewMessageReportInput {
                    reviewer_user_id: "another-moderator".to_string(),
                    status: pb::DirectMessageReportStatus::Rejected as i32,
                    resolution: "different outcome".to_string(),
                    action: pb::DirectMessageModerationAction::NoAction as i32,
                },
            )
            .await;
        let blocked_send = Dao
            .send(input(
                "sender",
                "other-recipient",
                "message-2",
                "another message",
            ))
            .await;

        assert_eq!(
            reviewed.action,
            pb::DirectMessageModerationAction::RestrictSender as i32
        );
        assert!(Dao.sender_restricted("sender").await.expect("restriction"));
        assert!(matches!(conflicting_review, Err(DaoError::ReportConflict)));
        assert!(matches!(blocked_send, Err(DaoError::SenderRestricted)));
    }
}

#[path = "memory_message_dao.rs"]
mod memory_message_dao;
pub(crate) use memory_message_dao::MemoryMessageDao;
#[path = "postgres_message_dao.rs"]
mod postgres_message_dao;
pub(crate) use postgres_message_dao::PostgresMessageDao;
