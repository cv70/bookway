use super::*;

#[derive(Default)]
pub(crate) struct MemoryMessageDao {
    pub(super) state: RwLock<MemoryState>,
}

#[async_trait]
impl MessageDao for MemoryMessageDao {
    async fn preferences(&self, user_id: &str) -> Result<pb::DirectMessagePreferences, DaoError> {
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
    ) -> Result<pb::DirectMessagePreferences, DaoError> {
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

    async fn send(&self, input: SendMessageInput) -> Result<pb::DirectMessage, DaoError> {
        let mut state = self.state.write().await;
        if state.restrictions.contains_key(&input.sender_user_id) {
            return Err(DaoError::SenderRestricted);
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
                .ok_or_else(|| DaoError::NotFound(message_id.clone()))?;
            if message.recipient_user_id == input.recipient_user_id
                && message.body == input.body
                && message.kind == input.kind
            {
                // Mirror the PostgreSQL outbox invariant for local development:
                // a retry can restore a missing task but never duplicates one.
                state.notification_message_ids.insert(message.id.clone());
                return Ok(message);
            }
            return Err(DaoError::IdempotencyConflict);
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

    async fn sender_restricted(&self, user_id: &str) -> Result<bool, DaoError> {
        Ok(self.state.read().await.restrictions.contains_key(user_id))
    }

    async fn list_conversations(
        &self,
        user_id: &str,
        cursor: Option<&ConversationCursor>,
        limit: usize,
    ) -> Result<Vec<pb::Conversation>, DaoError> {
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
    ) -> Result<Vec<pb::DirectMessage>, DaoError> {
        let state = self.state.read().await;
        let conversation = state
            .conversations
            .get(conversation_id)
            .ok_or_else(|| DaoError::NotFound(conversation_id.to_string()))?;
        if !conversation_has_user(conversation, user_id) {
            return Err(DaoError::NotParticipant);
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
    ) -> Result<pb::MarkConversationReadResponse, DaoError> {
        let mut state = self.state.write().await;
        let conversation = state
            .conversations
            .get(conversation_id)
            .ok_or_else(|| DaoError::NotFound(conversation_id.to_string()))?;
        if !conversation_has_user(conversation, user_id) {
            return Err(DaoError::NotParticipant);
        }
        let through = through_message_id
            .map(|id| {
                state
                    .messages
                    .iter()
                    .find(|message| message.id == id && message.conversation_id == conversation_id)
                    .ok_or_else(|| DaoError::NotFound(id.to_string()))
                    .and_then(|message| {
                        OffsetDateTime::parse(&message.created_at, &Rfc3339)
                            .map(|created_at| (created_at, message.id.clone()))
                            .map_err(|_| DaoError::NotFound(id.to_string()))
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
    ) -> Result<pb::DirectMessageReport, DaoError> {
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
                return Err(DaoError::ReportIdempotencyConflict);
            }
        }
        let message = state
            .messages
            .iter()
            .find(|message| message.id == input.message_id)
            .cloned()
            .ok_or_else(|| DaoError::MessageNotFound(input.message_id.clone()))?;
        if message.recipient_user_id != input.reporter_user_id {
            return Err(DaoError::NotMessageRecipient);
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
    ) -> Result<Vec<pb::DirectMessageReport>, DaoError> {
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
    ) -> Result<pb::DirectMessageReport, DaoError> {
        let mut state = self.state.write().await;
        let report = state
            .reports
            .get_mut(report_id)
            .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))?;
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
