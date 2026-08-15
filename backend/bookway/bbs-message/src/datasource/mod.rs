use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
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
pub(crate) trait MessageRepository: Send + Sync {
    async fn preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::DirectMessagePreferences, RepositoryError>;
    async fn update_preferences(
        &self,
        user_id: &str,
        allow_direct_messages: bool,
    ) -> Result<pb::DirectMessagePreferences, RepositoryError>;
    async fn send(&self, input: SendMessageInput) -> Result<pb::DirectMessage, RepositoryError>;
    async fn sender_restricted(&self, user_id: &str) -> Result<bool, RepositoryError>;
    async fn list_conversations(
        &self,
        user_id: &str,
        cursor: Option<&ConversationCursor>,
        limit: usize,
    ) -> Result<Vec<pb::Conversation>, RepositoryError>;
    async fn list_messages(
        &self,
        user_id: &str,
        conversation_id: &str,
        cursor: Option<&MessageCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessage>, RepositoryError>;
    async fn mark_read(
        &self,
        user_id: &str,
        conversation_id: &str,
        through_message_id: Option<&str>,
    ) -> Result<pb::MarkConversationReadResponse, RepositoryError>;
    async fn create_report(
        &self,
        input: CreateMessageReportInput,
    ) -> Result<pb::DirectMessageReport, RepositoryError>;
    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessageReport>, RepositoryError>;
    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewMessageReportInput,
    ) -> Result<pb::DirectMessageReport, RepositoryError>;
}

#[derive(Default)]
pub(crate) struct MemoryMessageRepository {
    state: RwLock<MemoryState>,
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

#[async_trait]
impl MessageRepository for MemoryMessageRepository {
    async fn preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::DirectMessagePreferences, RepositoryError> {
        Ok(self
            .state
            .read()
            .await
            .preferences
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| default_preferences(user_id)))
    }

    async fn update_preferences(
        &self,
        user_id: &str,
        allow_direct_messages: bool,
    ) -> Result<pb::DirectMessagePreferences, RepositoryError> {
        let preferences = pb::DirectMessagePreferences {
            user_id: user_id.to_string(),
            allow_direct_messages,
            updated_at: now_timestamp(),
        };
        self.state
            .write()
            .await
            .preferences
            .insert(user_id.to_string(), preferences.clone());
        Ok(preferences)
    }

    async fn send(&self, input: SendMessageInput) -> Result<pb::DirectMessage, RepositoryError> {
        let mut state = self.state.write().await;
        if state.restrictions.contains_key(&input.sender_user_id) {
            return Err(RepositoryError::SenderRestricted);
        }
        let request_key = (
            input.sender_user_id.clone(),
            input.client_message_id.clone(),
        );
        if let Some(message_id) = state.client_messages.get(&request_key) {
            let message = state
                .messages
                .iter()
                .find(|message| message.id == *message_id)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(message_id.clone()))?;
            if message.recipient_user_id == input.recipient_user_id
                && message.body == input.body
                && message.kind == input.kind
            {
                // Mirror the PostgreSQL outbox invariant for local development:
                // a retry can restore a missing task but never duplicates one.
                state.notification_message_ids.insert(message.id.clone());
                return Ok(message);
            }
            return Err(RepositoryError::IdempotencyConflict);
        }
        let conversation_id = conversation_id(&input.sender_user_id, &input.recipient_user_id);
        let (participant_one_id, participant_two_id) =
            sorted_participants(&input.sender_user_id, &input.recipient_user_id);
        let created_at = now_timestamp();
        let message = pb::DirectMessage {
            id: Uuid::now_v7().to_string(),
            conversation_id: conversation_id.clone(),
            sender_user_id: input.sender_user_id,
            recipient_user_id: input.recipient_user_id,
            kind: input.kind,
            body: input.body,
            created_at: created_at.clone(),
            read_at: None,
        };
        state.conversations.insert(
            conversation_id.clone(),
            MemoryConversation {
                id: conversation_id,
                participant_one_id,
                participant_two_id,
                last_message_id: message.id.clone(),
                last_message_at: created_at,
            },
        );
        state
            .client_messages
            .insert(request_key, message.id.clone());
        state.notification_message_ids.insert(message.id.clone());
        state.messages.push(message.clone());
        Ok(message)
    }

    async fn sender_restricted(&self, user_id: &str) -> Result<bool, RepositoryError> {
        Ok(self.state.read().await.restrictions.contains_key(user_id))
    }

    async fn list_conversations(
        &self,
        user_id: &str,
        cursor: Option<&ConversationCursor>,
        limit: usize,
    ) -> Result<Vec<pb::Conversation>, RepositoryError> {
        let state = self.state.read().await;
        let mut conversations = state
            .conversations
            .values()
            .filter(|conversation| conversation_has_user(conversation, user_id))
            .filter_map(|conversation| {
                let last_message = state
                    .messages
                    .iter()
                    .find(|message| message.id == conversation.last_message_id)?;
                let last_message_at =
                    OffsetDateTime::parse(&conversation.last_message_at, &Rfc3339).ok()?;
                if cursor.is_some_and(|cursor| {
                    (last_message_at, conversation.id.as_str())
                        >= (cursor.last_message_at, cursor.id.as_str())
                }) {
                    return None;
                }
                let unread_count = state
                    .messages
                    .iter()
                    .filter(|message| {
                        message.conversation_id == conversation.id
                            && message.recipient_user_id == user_id
                            && message.read_at.is_none()
                    })
                    .count() as u64;
                Some(pb::Conversation {
                    id: conversation.id.clone(),
                    peer_user_id: peer_user_id(conversation, user_id),
                    last_message_preview: preview(&last_message.body),
                    last_message_at: conversation.last_message_at.clone(),
                    unread_count,
                })
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|left, right| {
            let left_at = OffsetDateTime::parse(&left.last_message_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            let right_at = OffsetDateTime::parse(&right.last_message_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            right_at.cmp(&left_at).then_with(|| right.id.cmp(&left.id))
        });
        conversations.truncate(limit);
        Ok(conversations)
    }

    async fn list_messages(
        &self,
        user_id: &str,
        conversation_id: &str,
        cursor: Option<&MessageCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessage>, RepositoryError> {
        let state = self.state.read().await;
        let conversation = state
            .conversations
            .get(conversation_id)
            .ok_or_else(|| RepositoryError::NotFound(conversation_id.to_string()))?;
        if !conversation_has_user(conversation, user_id) {
            return Err(RepositoryError::NotParticipant);
        }
        let mut messages = state
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .filter(|message| {
                cursor.is_none_or(|cursor| {
                    OffsetDateTime::parse(&message.created_at, &Rfc3339).is_ok_and(|created_at| {
                        (created_at, message.id.as_str()) < (cursor.created_at, cursor.id.as_str())
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        messages.sort_by(|left, right| {
            let left_at = OffsetDateTime::parse(&left.created_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            let right_at = OffsetDateTime::parse(&right.created_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            right_at.cmp(&left_at).then_with(|| right.id.cmp(&left.id))
        });
        messages.truncate(limit);
        messages.reverse();
        Ok(messages)
    }

    async fn mark_read(
        &self,
        user_id: &str,
        conversation_id: &str,
        through_message_id: Option<&str>,
    ) -> Result<pb::MarkConversationReadResponse, RepositoryError> {
        let mut state = self.state.write().await;
        let conversation = state
            .conversations
            .get(conversation_id)
            .ok_or_else(|| RepositoryError::NotFound(conversation_id.to_string()))?;
        if !conversation_has_user(conversation, user_id) {
            return Err(RepositoryError::NotParticipant);
        }
        let through = through_message_id
            .map(|id| {
                state
                    .messages
                    .iter()
                    .find(|message| message.id == id && message.conversation_id == conversation_id)
                    .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
                    .and_then(|message| {
                        OffsetDateTime::parse(&message.created_at, &Rfc3339)
                            .map(|created_at| (created_at, message.id.clone()))
                            .map_err(|_| RepositoryError::NotFound(id.to_string()))
                    })
            })
            .transpose()?;
        let read_at = now_timestamp();
        let mut marked_count = 0_u64;
        for message in &mut state.messages {
            let is_before_through = through.as_ref().is_none_or(|(through_at, through_id)| {
                OffsetDateTime::parse(&message.created_at, &Rfc3339).is_ok_and(|created_at| {
                    (created_at, message.id.as_str()) <= (*through_at, through_id.as_str())
                })
            });
            if message.conversation_id == conversation_id
                && message.recipient_user_id == user_id
                && message.read_at.is_none()
                && is_before_through
            {
                message.read_at = Some(read_at.clone());
                marked_count += 1;
            }
        }
        Ok(pb::MarkConversationReadResponse {
            marked_count,
            read_at,
        })
    }

    async fn create_report(
        &self,
        input: CreateMessageReportInput,
    ) -> Result<pb::DirectMessageReport, RepositoryError> {
        let mut state = self.state.write().await;
        if let Some(idempotency_key) = &input.idempotency_key {
            let request_key = (input.reporter_user_id.clone(), idempotency_key.clone());
            if let Some(report_id) = state.report_idempotency.get(&request_key)
                && let Some(report) = state.reports.get(report_id)
            {
                if report
                    .reported_message
                    .as_ref()
                    .is_some_and(|message| message.id == input.message_id)
                    && report.reason == input.reason
                    && report.details == input.details
                {
                    return Ok(report.clone());
                }
                return Err(RepositoryError::ReportIdempotencyConflict);
            }
        }
        let message = state
            .messages
            .iter()
            .find(|message| message.id == input.message_id)
            .cloned()
            .ok_or_else(|| RepositoryError::MessageNotFound(input.message_id.clone()))?;
        if message.recipient_user_id != input.reporter_user_id {
            return Err(RepositoryError::NotMessageRecipient);
        }
        let report = pb::DirectMessageReport {
            id: input.id.clone(),
            reporter_user_id: input.reporter_user_id.clone(),
            reported_user_id: message.sender_user_id.clone(),
            reported_message: Some(message),
            reason: input.reason,
            details: input.details,
            status: pb::DirectMessageReportStatus::Pending as i32,
            reviewer_user_id: None,
            resolution: None,
            action: pb::DirectMessageModerationAction::NoAction as i32,
            created_at: input.created_at.clone(),
            updated_at: input.created_at,
        };
        if let Some(idempotency_key) = input.idempotency_key {
            state
                .report_idempotency
                .insert((input.reporter_user_id, idempotency_key), report.id.clone());
        }
        state.reports.insert(report.id.clone(), report.clone());
        Ok(report)
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessageReport>, RepositoryError> {
        let state = self.state.read().await;
        let mut reports = state
            .reports
            .values()
            .filter(|report| status.is_none_or(|status| report.status == status))
            .filter(|report| {
                cursor.is_none_or(|cursor| {
                    ReportCursor::from_report(report).is_some_and(|value| {
                        (value.created_at, value.id.as_str())
                            > (cursor.created_at, cursor.id.as_str())
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        reports.sort_by(|left, right| {
            ReportCursor::from_report(left).cmp(&ReportCursor::from_report(right))
        });
        reports.truncate(limit);
        Ok(reports)
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewMessageReportInput,
    ) -> Result<pb::DirectMessageReport, RepositoryError> {
        let mut state = self.state.write().await;
        let report = state
            .reports
            .get_mut(report_id)
            .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))?;
        let reviewed = apply_report_review(report, &input)?;
        if reviewed.status == pb::DirectMessageReportStatus::Resolved as i32
            && reviewed.action == pb::DirectMessageModerationAction::RestrictSender as i32
        {
            state
                .restrictions
                .insert(reviewed.reported_user_id.clone(), reviewed.id.clone());
        }
        Ok(reviewed)
    }
}

pub(crate) struct PostgresMessageRepository {
    pool: sqlx::PgPool,
}

impl PostgresMessageRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct PreferenceRow {
    user_id: String,
    allow_direct_messages: bool,
    updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: String,
    peer_user_id: String,
    last_message_preview: String,
    last_message_at: OffsetDateTime,
    unread_count: i64,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    conversation_id: String,
    sender_user_id: String,
    recipient_user_id: String,
    kind: String,
    body: String,
    created_at: OffsetDateTime,
    read_at: Option<OffsetDateTime>,
}

#[derive(sqlx::FromRow)]
struct ReportRow {
    report_id: String,
    reporter_user_id: String,
    reported_user_id: String,
    reason: String,
    details: String,
    status: String,
    reviewer_user_id: Option<String>,
    resolution: Option<String>,
    action: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    message_id: String,
    conversation_id: String,
    sender_user_id: String,
    recipient_user_id: String,
    kind: String,
    body: String,
    message_created_at: OffsetDateTime,
    read_at: Option<OffsetDateTime>,
}

#[async_trait]
impl MessageRepository for PostgresMessageRepository {
    async fn preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::DirectMessagePreferences, RepositoryError> {
        sqlx::query_as::<_, PreferenceRow>(
            "SELECT user_id,allow_direct_messages,updated_at FROM direct_message_preferences WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .map(PreferenceRow::into_preferences)
        .transpose()?
        .map_or_else(|| Ok(default_preferences(user_id)), Ok)
    }

    async fn update_preferences(
        &self,
        user_id: &str,
        allow_direct_messages: bool,
    ) -> Result<pb::DirectMessagePreferences, RepositoryError> {
        sqlx::query_as::<_, PreferenceRow>(
            "INSERT INTO direct_message_preferences (user_id,allow_direct_messages) VALUES ($1,$2) ON CONFLICT (user_id) DO UPDATE SET allow_direct_messages = EXCLUDED.allow_direct_messages,updated_at = now() RETURNING user_id,allow_direct_messages,updated_at",
        )
        .bind(user_id)
        .bind(allow_direct_messages)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .into_preferences()
    }

    async fn send(&self, input: SendMessageInput) -> Result<pb::DirectMessage, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if sender_restricted_in_transaction(&mut transaction, &input.sender_user_id).await? {
            return Err(RepositoryError::SenderRestricted);
        }
        let (participant_one_id, participant_two_id) =
            sorted_participants(&input.sender_user_id, &input.recipient_user_id);
        let conversation_id = conversation_id(&participant_one_id, &participant_two_id);
        sqlx::query(
            "INSERT INTO direct_conversations (id,participant_one_id,participant_two_id) VALUES ($1,$2,$3) ON CONFLICT (participant_one_id,participant_two_id) DO NOTHING",
        )
        .bind(&conversation_id)
        .bind(&participant_one_id)
        .bind(&participant_two_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;

        let message_id = Uuid::now_v7().to_string();
        let inserted = sqlx::query_as::<_, MessageRow>(
            "INSERT INTO direct_messages (id,conversation_id,sender_user_id,recipient_user_id,client_message_id,kind,body) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (sender_user_id,client_message_id) DO NOTHING RETURNING id,conversation_id,sender_user_id,recipient_user_id,kind,body,created_at,read_at",
        )
        .bind(&message_id)
        .bind(&conversation_id)
        .bind(&input.sender_user_id)
        .bind(&input.recipient_user_id)
        .bind(&input.client_message_id)
        .bind(kind_name(input.kind)?)
        .bind(&input.body)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let Some(row) = inserted else {
            let existing = sqlx::query_as::<_, MessageRow>(
                "SELECT id,conversation_id,sender_user_id,recipient_user_id,kind,body,created_at,read_at FROM direct_messages WHERE sender_user_id = $1 AND client_message_id = $2",
            )
            .bind(&input.sender_user_id)
            .bind(&input.client_message_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if existing.recipient_user_id != input.recipient_user_id
                || existing.body != input.body
                || parse_kind(&existing.kind)? != input.kind
            {
                return Err(RepositoryError::IdempotencyConflict);
            }
            enqueue_direct_message_notification(&mut transaction, &existing).await?;
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return existing.into_message();
        };
        sqlx::query(
            "UPDATE direct_conversations SET last_message_id = $2,last_message_at = $3,updated_at = now() WHERE id = $1 AND (last_message_at IS NULL OR last_message_at <= $3)",
        )
        .bind(&conversation_id)
        .bind(&row.id)
        .bind(row.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        enqueue_direct_message_notification(&mut transaction, &row).await?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        row.into_message()
    }

    async fn sender_restricted(&self, user_id: &str) -> Result<bool, RepositoryError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM direct_message_restrictions WHERE sender_user_id = $1)",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::Database)
    }

    async fn list_conversations(
        &self,
        user_id: &str,
        cursor: Option<&ConversationCursor>,
        limit: usize,
    ) -> Result<Vec<pb::Conversation>, RepositoryError> {
        let rows = sqlx::query_as::<_, ConversationRow>(
            "SELECT c.id,CASE WHEN c.participant_one_id = $1 THEN c.participant_two_id ELSE c.participant_one_id END AS peer_user_id,LEFT(last_message.body,120) AS last_message_preview,c.last_message_at,COUNT(unread.id) AS unread_count FROM direct_conversations AS c JOIN direct_messages AS last_message ON last_message.id = c.last_message_id LEFT JOIN direct_messages AS unread ON unread.conversation_id = c.id AND unread.recipient_user_id = $1 AND unread.read_at IS NULL WHERE ($1 = c.participant_one_id OR $1 = c.participant_two_id) AND ($2::TIMESTAMPTZ IS NULL OR (c.last_message_at,c.id) < ($2,$3)) GROUP BY c.id,last_message.body,c.last_message_at ORDER BY c.last_message_at DESC,c.id DESC LIMIT $4",
        )
        .bind(user_id)
        .bind(cursor.map(|value| value.last_message_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(ConversationRow::into_conversation)
            .collect()
    }

    async fn list_messages(
        &self,
        user_id: &str,
        conversation_id: &str,
        cursor: Option<&MessageCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessage>, RepositoryError> {
        ensure_participant(&self.pool, user_id, conversation_id).await?;
        let mut rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id,conversation_id,sender_user_id,recipient_user_id,kind,body,created_at,read_at FROM direct_messages WHERE conversation_id = $1 AND ($2::TIMESTAMPTZ IS NULL OR (created_at,id) < ($2,$3)) ORDER BY created_at DESC,id DESC LIMIT $4",
        )
        .bind(conversation_id)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.reverse();
        rows.into_iter().map(MessageRow::into_message).collect()
    }

    async fn mark_read(
        &self,
        user_id: &str,
        conversation_id: &str,
        through_message_id: Option<&str>,
    ) -> Result<pb::MarkConversationReadResponse, RepositoryError> {
        ensure_participant(&self.pool, user_id, conversation_id).await?;
        let through = match through_message_id {
            Some(message_id) => Some(
                sqlx::query_as::<_, (OffsetDateTime, String)>(
                    "SELECT created_at,id FROM direct_messages WHERE id = $1 AND conversation_id = $2",
                )
                .bind(message_id)
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(RepositoryError::Database)?
                .ok_or_else(|| RepositoryError::NotFound(message_id.to_string()))?,
            ),
            None => None,
        };
        let read_at = OffsetDateTime::now_utc();
        let result = sqlx::query(
            "UPDATE direct_messages SET read_at = $1 WHERE conversation_id = $2 AND recipient_user_id = $3 AND read_at IS NULL AND ($4::TIMESTAMPTZ IS NULL OR (created_at,id) <= ($4,$5))",
        )
        .bind(read_at)
        .bind(conversation_id)
        .bind(user_id)
        .bind(through.as_ref().map(|value| value.0))
        .bind(through.as_ref().map(|value| value.1.as_str()))
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(pb::MarkConversationReadResponse {
            marked_count: result.rows_affected(),
            read_at: format_timestamp(read_at),
        })
    }

    async fn create_report(
        &self,
        input: CreateMessageReportInput,
    ) -> Result<pb::DirectMessageReport, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let message = select_message_for_report(&mut transaction, &input.message_id).await?;
        if message.recipient_user_id != input.reporter_user_id {
            return Err(RepositoryError::NotMessageRecipient);
        }
        if let Some(idempotency_key) = &input.idempotency_key {
            let inserted = sqlx::query_scalar::<_, String>(
                "INSERT INTO direct_message_reports (id,message_id,reporter_user_id,reported_user_id,reason,details,idempotency_key,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8::TIMESTAMPTZ,$8::TIMESTAMPTZ) ON CONFLICT (reporter_user_id,idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING RETURNING id",
            )
            .bind(&input.id)
            .bind(&input.message_id)
            .bind(&input.reporter_user_id)
            .bind(&message.sender_user_id)
            .bind(report_reason_name(input.reason)?)
            .bind(&input.details)
            .bind(idempotency_key)
            .bind(&input.created_at)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if let Some(report_id) = inserted {
                let report = select_report(&mut transaction, &report_id).await?;
                transaction
                    .commit()
                    .await
                    .map_err(RepositoryError::Database)?;
                return report.into_report();
            }
            let existing = select_report_by_idempotency(
                &mut transaction,
                &input.reporter_user_id,
                idempotency_key,
            )
            .await?;
            if existing.message_id != input.message_id
                || parse_report_reason(&existing.reason)? != input.reason
                || existing.details != input.details
            {
                return Err(RepositoryError::ReportIdempotencyConflict);
            }
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return existing.into_report();
        }
        sqlx::query(
            "INSERT INTO direct_message_reports (id,message_id,reporter_user_id,reported_user_id,reason,details,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7::TIMESTAMPTZ,$7::TIMESTAMPTZ)",
        )
        .bind(&input.id)
        .bind(&input.message_id)
        .bind(&input.reporter_user_id)
        .bind(&message.sender_user_id)
        .bind(report_reason_name(input.reason)?)
        .bind(&input.details)
        .bind(&input.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let report = select_report(&mut transaction, &input.id).await?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        report.into_report()
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessageReport>, RepositoryError> {
        let rows = sqlx::query_as::<_, ReportRow>(
            "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE ($1::TEXT IS NULL OR r.status = $1) AND ($2::TIMESTAMPTZ IS NULL OR (r.created_at,r.id) > ($2,$3)) ORDER BY r.created_at ASC,r.id ASC LIMIT $4",
        )
        .bind(status.map(report_status_name).transpose()?)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter().map(ReportRow::into_report).collect()
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewMessageReportInput,
    ) -> Result<pb::DirectMessageReport, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let row = select_report_for_update(&mut transaction, report_id).await?;
        let mut report = row.into_report()?;
        let was_terminal = is_terminal_report(report.status);
        let reviewed = apply_report_review(&mut report, &input)?;
        if was_terminal {
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return Ok(reviewed);
        }
        let updated_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "UPDATE direct_message_reports SET status = $2,reviewer_user_id = $3,resolution = $4,action = $5,updated_at = now() WHERE id = $1 RETURNING updated_at",
        )
        .bind(report_id)
        .bind(report_status_name(reviewed.status)?)
        .bind(&reviewed.reviewer_user_id)
        .bind(&reviewed.resolution)
        .bind(moderation_action_name(reviewed.action)?)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if reviewed.status == pb::DirectMessageReportStatus::Resolved as i32
            && reviewed.action == pb::DirectMessageModerationAction::RestrictSender as i32
        {
            sqlx::query(
                "INSERT INTO direct_message_restrictions (sender_user_id,report_id,reviewer_user_id,resolution) VALUES ($1,$2,$3,$4) ON CONFLICT (sender_user_id) DO NOTHING",
            )
            .bind(&reviewed.reported_user_id)
            .bind(report_id)
            .bind(&input.reviewer_user_id)
            .bind(&input.resolution)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        report.updated_at = format_timestamp(updated_at);
        Ok(report)
    }
}

impl PreferenceRow {
    fn into_preferences(self) -> Result<pb::DirectMessagePreferences, RepositoryError> {
        Ok(pb::DirectMessagePreferences {
            user_id: self.user_id,
            allow_direct_messages: self.allow_direct_messages,
            updated_at: format_timestamp(self.updated_at),
        })
    }
}

impl ConversationRow {
    fn into_conversation(self) -> Result<pb::Conversation, RepositoryError> {
        Ok(pb::Conversation {
            id: self.id,
            peer_user_id: self.peer_user_id,
            last_message_preview: self.last_message_preview,
            last_message_at: format_timestamp(self.last_message_at),
            unread_count: self.unread_count.max(0) as u64,
        })
    }
}

impl MessageRow {
    fn into_message(self) -> Result<pb::DirectMessage, RepositoryError> {
        Ok(pb::DirectMessage {
            id: self.id,
            conversation_id: self.conversation_id,
            sender_user_id: self.sender_user_id,
            recipient_user_id: self.recipient_user_id,
            kind: parse_kind(&self.kind)?,
            body: self.body,
            created_at: format_timestamp(self.created_at),
            read_at: self.read_at.map(format_timestamp),
        })
    }
}

impl ReportRow {
    fn into_report(self) -> Result<pb::DirectMessageReport, RepositoryError> {
        Ok(pb::DirectMessageReport {
            id: self.report_id,
            reporter_user_id: self.reporter_user_id,
            reported_user_id: self.reported_user_id,
            reported_message: Some(pb::DirectMessage {
                id: self.message_id,
                conversation_id: self.conversation_id,
                sender_user_id: self.sender_user_id,
                recipient_user_id: self.recipient_user_id,
                kind: parse_kind(&self.kind)?,
                body: self.body,
                created_at: format_timestamp(self.message_created_at),
                read_at: self.read_at.map(format_timestamp),
            }),
            reason: parse_report_reason(&self.reason)?,
            details: self.details,
            status: parse_report_status(&self.status)?,
            reviewer_user_id: self.reviewer_user_id,
            resolution: self.resolution,
            action: parse_moderation_action(&self.action)?,
            created_at: format_timestamp(self.created_at),
            updated_at: format_timestamp(self.updated_at),
        })
    }
}

async fn sender_restricted_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
) -> Result<bool, RepositoryError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM direct_message_restrictions WHERE sender_user_id = $1)",
    )
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)
}

async fn enqueue_direct_message_notification(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    message: &MessageRow,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO direct_message_notification_jobs (message_id,conversation_id,recipient_user_id,sender_user_id) VALUES ($1,$2,$3,$4) ON CONFLICT (message_id) DO NOTHING",
    )
    .bind(&message.id)
    .bind(&message.conversation_id)
    .bind(&message.recipient_user_id)
    .bind(&message.sender_user_id)
    .execute(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    Ok(())
}

async fn select_message_for_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    message_id: &str,
) -> Result<MessageRow, RepositoryError> {
    sqlx::query_as::<_, MessageRow>(
        "SELECT id,conversation_id,sender_user_id,recipient_user_id,kind,body,created_at,read_at FROM direct_messages WHERE id = $1",
    )
    .bind(message_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::MessageNotFound(message_id.to_string()))
}

async fn select_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<ReportRow, RepositoryError> {
    sqlx::query_as::<_, ReportRow>(
        "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE r.id = $1",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))
}

async fn select_report_by_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    reporter_user_id: &str,
    idempotency_key: &str,
) -> Result<ReportRow, RepositoryError> {
    sqlx::query_as::<_, ReportRow>(
        "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE r.reporter_user_id = $1 AND r.idempotency_key = $2",
    )
    .bind(reporter_user_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::ReportIdempotencyConflict)
}

async fn select_report_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<ReportRow, RepositoryError> {
    sqlx::query_as::<_, ReportRow>(
        "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE r.id = $1 FOR UPDATE OF r",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::ReportNotFound(report_id.to_string()))
}

async fn ensure_participant(
    pool: &sqlx::PgPool,
    user_id: &str,
    conversation_id: &str,
) -> Result<(), RepositoryError> {
    let participants = sqlx::query_as::<_, (String, String)>(
        "SELECT participant_one_id,participant_two_id FROM direct_conversations WHERE id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or_else(|| RepositoryError::NotFound(conversation_id.to_string()))?;
    if participants.0 != user_id && participants.1 != user_id {
        return Err(RepositoryError::NotParticipant);
    }
    Ok(())
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

fn kind_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::DirectMessageKind::try_from(value) {
        Ok(pb::DirectMessageKind::Text) => Ok("text"),
        Err(_) => Err(RepositoryError::InvalidKind(value.to_string())),
    }
}

fn parse_kind(value: &str) -> Result<i32, RepositoryError> {
    match value {
        "text" => Ok(pb::DirectMessageKind::Text as i32),
        value => Err(RepositoryError::InvalidKind(value.to_string())),
    }
}

fn report_reason_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::DirectMessageReportReason::try_from(value) {
        Ok(pb::DirectMessageReportReason::Spam) => Ok("spam"),
        Ok(pb::DirectMessageReportReason::Harassment) => Ok("harassment"),
        Ok(pb::DirectMessageReportReason::Unsafe) => Ok("unsafe"),
        Ok(pb::DirectMessageReportReason::Fraud) => Ok("fraud"),
        Ok(pb::DirectMessageReportReason::Privacy) => Ok("privacy"),
        Ok(pb::DirectMessageReportReason::Other) => Ok("other"),
        Err(_) => Err(RepositoryError::InvalidReportValue {
            field: "reason",
            value: value.to_string(),
        }),
    }
}

fn parse_report_reason(value: &str) -> Result<i32, RepositoryError> {
    let reason = match value {
        "spam" => pb::DirectMessageReportReason::Spam,
        "harassment" => pb::DirectMessageReportReason::Harassment,
        "unsafe" => pb::DirectMessageReportReason::Unsafe,
        "fraud" => pb::DirectMessageReportReason::Fraud,
        "privacy" => pb::DirectMessageReportReason::Privacy,
        "other" => pb::DirectMessageReportReason::Other,
        value => {
            return Err(RepositoryError::InvalidReportValue {
                field: "reason",
                value: value.to_string(),
            });
        }
    };
    Ok(reason as i32)
}

fn report_status_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::DirectMessageReportStatus::try_from(value) {
        Ok(pb::DirectMessageReportStatus::Pending) => Ok("pending"),
        Ok(pb::DirectMessageReportStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::DirectMessageReportStatus::Resolved) => Ok("resolved"),
        Ok(pb::DirectMessageReportStatus::Rejected) => Ok("rejected"),
        Err(_) => Err(RepositoryError::InvalidReportValue {
            field: "status",
            value: value.to_string(),
        }),
    }
}

fn parse_report_status(value: &str) -> Result<i32, RepositoryError> {
    let status = match value {
        "pending" => pb::DirectMessageReportStatus::Pending,
        "reviewing" => pb::DirectMessageReportStatus::Reviewing,
        "resolved" => pb::DirectMessageReportStatus::Resolved,
        "rejected" => pb::DirectMessageReportStatus::Rejected,
        value => {
            return Err(RepositoryError::InvalidReportValue {
                field: "status",
                value: value.to_string(),
            });
        }
    };
    Ok(status as i32)
}

fn moderation_action_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::DirectMessageModerationAction::try_from(value) {
        Ok(pb::DirectMessageModerationAction::NoAction) => Ok("no_action"),
        Ok(pb::DirectMessageModerationAction::RestrictSender) => Ok("restrict_sender"),
        Err(_) => Err(RepositoryError::InvalidReportValue {
            field: "action",
            value: value.to_string(),
        }),
    }
}

fn parse_moderation_action(value: &str) -> Result<i32, RepositoryError> {
    let action = match value {
        "no_action" => pb::DirectMessageModerationAction::NoAction,
        "restrict_sender" => pb::DirectMessageModerationAction::RestrictSender,
        value => {
            return Err(RepositoryError::InvalidReportValue {
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
) -> Result<pb::DirectMessageReport, RepositoryError> {
    let status = pb::DirectMessageReportStatus::try_from(input.status).map_err(|_| {
        RepositoryError::InvalidReportValue {
            field: "status",
            value: input.status.to_string(),
        }
    })?;
    let action = pb::DirectMessageModerationAction::try_from(input.action).map_err(|_| {
        RepositoryError::InvalidReportValue {
            field: "action",
            value: input.action.to_string(),
        }
    })?;
    if is_terminal_report(report.status) {
        return (report.status == input.status
            && report.resolution.as_deref() == Some(input.resolution.as_str())
            && report.action == input.action)
            .then(|| report.clone())
            .ok_or(RepositoryError::ReportConflict);
    }
    if status == pb::DirectMessageReportStatus::Pending {
        return Err(RepositoryError::InvalidReportValue {
            field: "status",
            value: "pending is not a review decision".to_string(),
        });
    }
    if status == pb::DirectMessageReportStatus::Reviewing
        && (!input.resolution.is_empty() || action != pb::DirectMessageModerationAction::NoAction)
    {
        return Err(RepositoryError::InvalidReportValue {
            field: "review",
            value: "reviewing reports cannot resolve or restrict a sender".to_string(),
        });
    }
    if is_terminal_report(input.status) && input.resolution.is_empty() {
        return Err(RepositoryError::InvalidReportValue {
            field: "resolution",
            value: "terminal reviews require a resolution".to_string(),
        });
    }
    if status == pb::DirectMessageReportStatus::Rejected
        && action != pb::DirectMessageModerationAction::NoAction
    {
        return Err(RepositoryError::InvalidReportValue {
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
        let repository = MemoryMessageRepository::default();
        let first = repository
            .send(input("reader-a", "creator-b", "client-1", "你好"))
            .await
            .expect("initial send");
        let retry = repository
            .send(input("reader-a", "creator-b", "client-1", "你好"))
            .await
            .expect("retry");
        let conflict = repository
            .send(input("reader-a", "creator-b", "client-1", "另一条正文"))
            .await;

        assert_eq!(first.id, retry.id);
        assert!(matches!(
            conflict,
            Err(RepositoryError::IdempotencyConflict)
        ));
        let state = repository.state.read().await;
        assert_eq!(state.notification_message_ids.len(), 1);
        assert!(state.notification_message_ids.contains(&first.id));
    }

    #[tokio::test]
    async fn reading_marks_only_recipient_messages_and_conversation_is_visible_to_both_sides() {
        let repository = MemoryMessageRepository::default();
        let first = repository
            .send(input("reader-a", "creator-b", "client-1", "第一条"))
            .await
            .expect("first message");
        repository
            .send(input("creator-b", "reader-a", "client-2", "收到"))
            .await
            .expect("reply");

        let creator_page = repository
            .list_conversations("creator-b", None, 10)
            .await
            .expect("creator conversations");
        assert_eq!(creator_page.len(), 1);
        assert_eq!(creator_page[0].peer_user_id, "reader-a");
        assert_eq!(creator_page[0].unread_count, 1);

        let read = repository
            .mark_read("creator-b", &first.conversation_id, Some(&first.id))
            .await
            .expect("mark first message read");
        assert_eq!(read.marked_count, 1);
        let reader_page = repository
            .list_conversations("reader-a", None, 10)
            .await
            .expect("reader conversations");
        assert_eq!(reader_page[0].unread_count, 1);
        let creator_page = repository
            .list_conversations("creator-b", None, 10)
            .await
            .expect("creator conversations after read");
        assert_eq!(creator_page[0].unread_count, 0);
    }

    #[tokio::test]
    async fn message_pages_are_chronological_and_continue_from_the_oldest_item() {
        let repository = MemoryMessageRepository::default();
        let mut messages = Vec::new();
        for index in 0..4 {
            messages.push(
                repository
                    .send(input(
                        "reader-a",
                        "creator-b",
                        &format!("client-{index}"),
                        &format!("message-{index}"),
                    ))
                    .await
                    .expect("send message"),
            );
        }
        let page = repository
            .list_messages("reader-a", &messages[0].conversation_id, None, 2)
            .await
            .expect("first page");
        assert_eq!(page.len(), 2);
        assert!(page[0].created_at <= page[1].created_at);
        let cursor = MessageCursor::from_message(&page[0]).expect("cursor");
        let older = repository
            .list_messages("reader-a", &messages[0].conversation_id, Some(&cursor), 2)
            .await
            .expect("older page");
        assert!(older.iter().all(|message| message.id != page[0].id));
    }

    #[tokio::test]
    async fn only_the_recipient_can_report_and_retries_are_idempotent() {
        let repository = MemoryMessageRepository::default();
        let message = repository
            .send(input(
                "sender",
                "recipient",
                "message-1",
                "unwanted message",
            ))
            .await
            .expect("send message");

        let report = repository
            .create_report(report_input(
                "recipient",
                &message.id,
                "report-1",
                "repeated abuse",
            ))
            .await
            .expect("recipient report");
        let retry = repository
            .create_report(report_input(
                "recipient",
                &message.id,
                "report-1",
                "repeated abuse",
            ))
            .await
            .expect("report retry");
        let sender_attempt = repository
            .create_report(report_input(
                "sender",
                &message.id,
                "report-2",
                "not allowed",
            ))
            .await;
        let conflicting_retry = repository
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
        assert!(matches!(
            sender_attempt,
            Err(RepositoryError::NotMessageRecipient)
        ));
        assert!(matches!(
            conflicting_retry,
            Err(RepositoryError::ReportIdempotencyConflict)
        ));
    }

    #[tokio::test]
    async fn resolved_restrictions_block_future_messages_and_terminal_reviews_conflict() {
        let repository = MemoryMessageRepository::default();
        let message = repository
            .send(input("sender", "recipient", "message-1", "unsafe message"))
            .await
            .expect("send message");
        let report = repository
            .create_report(report_input("recipient", &message.id, "report-1", "unsafe"))
            .await
            .expect("report message");
        let reviewed = repository
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
        let conflicting_review = repository
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
        let blocked_send = repository
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
        assert!(
            repository
                .sender_restricted("sender")
                .await
                .expect("restriction")
        );
        assert!(matches!(
            conflicting_review,
            Err(RepositoryError::ReportConflict)
        ));
        assert!(matches!(
            blocked_send,
            Err(RepositoryError::SenderRestricted)
        ));
    }
}
