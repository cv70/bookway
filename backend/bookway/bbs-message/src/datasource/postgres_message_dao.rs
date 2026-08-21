use super::*;

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

impl PreferenceRow {
    fn into_preferences(self) -> Result<pb::DirectMessagePreferences, DaoError> {
        Ok(pb::DirectMessagePreferences {
            user_id: self.user_id,
            allow_direct_messages: self.allow_direct_messages,
            updated_at: format_timestamp(self.updated_at),
        })
    }
}

impl ConversationRow {
    fn into_conversation(self) -> Result<pb::Conversation, DaoError> {
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
    fn into_message(self) -> Result<pb::DirectMessage, DaoError> {
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
    fn into_report(self) -> Result<pb::DirectMessageReport, DaoError> {
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
) -> Result<bool, DaoError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM direct_message_restrictions WHERE sender_user_id = $1)",
    )
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(DaoError::Database)
}

async fn enqueue_direct_message_notification(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    message: &MessageRow,
) -> Result<(), DaoError> {
    sqlx::query(
        "INSERT INTO direct_message_notification_jobs (message_id,conversation_id,recipient_user_id,sender_user_id) VALUES ($1,$2,$3,$4) ON CONFLICT (message_id) DO NOTHING",
    )
    .bind(&message.id)
    .bind(&message.conversation_id)
    .bind(&message.recipient_user_id)
    .bind(&message.sender_user_id)
    .execute(&mut **transaction)
    .await
    .map_err(DaoError::Database)?;
    Ok(())
}

async fn select_message_for_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    message_id: &str,
) -> Result<MessageRow, DaoError> {
    sqlx::query_as::<_, MessageRow>(
        "SELECT id,conversation_id,sender_user_id,recipient_user_id,kind,body,created_at,read_at FROM direct_messages WHERE id = $1",
    )
    .bind(message_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::MessageNotFound(message_id.to_string()))
}

async fn select_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<ReportRow, DaoError> {
    sqlx::query_as::<_, ReportRow>(
        "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE r.id = $1",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))
}

async fn select_report_by_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    reporter_user_id: &str,
    idempotency_key: &str,
) -> Result<ReportRow, DaoError> {
    sqlx::query_as::<_, ReportRow>(
        "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE r.reporter_user_id = $1 AND r.idempotency_key = $2",
    )
    .bind(reporter_user_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::ReportIdempotencyConflict)
}

async fn select_report_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    report_id: &str,
) -> Result<ReportRow, DaoError> {
    sqlx::query_as::<_, ReportRow>(
        "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE r.id = $1 FOR UPDATE OF r",
    )
    .bind(report_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))
}

async fn ensure_participant(
    pool: &sqlx::PgPool,
    user_id: &str,
    conversation_id: &str,
) -> Result<(), DaoError> {
    let participants = sqlx::query_as::<_, (String, String)>(
        "SELECT participant_one_id,participant_two_id FROM direct_conversations WHERE id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .map_err(DaoError::Database)?
    .ok_or_else(|| DaoError::NotFound(conversation_id.to_string()))?;
    if participants.0 != user_id && participants.1 != user_id {
        return Err(DaoError::NotParticipant);
    }
    Ok(())
}

pub(crate) struct PostgresMessageDao {
    pool: sqlx::PgPool,
}

impl PostgresMessageDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MessageDao for PostgresMessageDao {
    async fn preferences(&self, user_id: &str) -> Result<pb::DirectMessagePreferences, DaoError> {
        sqlx::query_as::<_, PreferenceRow>(
            "SELECT user_id,allow_direct_messages,updated_at FROM direct_message_preferences WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .map(PreferenceRow::into_preferences)
        .transpose()?
        .map_or_else(|| Ok(default_preferences(user_id)), Ok)
    }

    async fn update_preferences(
        &self,
        user_id: &str,
        allow_direct_messages: bool,
    ) -> Result<pb::DirectMessagePreferences, DaoError> {
        sqlx::query_as::<_, PreferenceRow>(
            "INSERT INTO direct_message_preferences (user_id,allow_direct_messages) VALUES ($1,$2) ON CONFLICT (user_id) DO UPDATE SET allow_direct_messages = EXCLUDED.allow_direct_messages,updated_at = now() RETURNING user_id,allow_direct_messages,updated_at",
        )
        .bind(user_id)
        .bind(allow_direct_messages)
        .fetch_one(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .into_preferences()
    }

    async fn send(&self, input: SendMessageInput) -> Result<pb::DirectMessage, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        if sender_restricted_in_transaction(&mut transaction, &input.sender_user_id).await? {
            return Err(DaoError::SenderRestricted);
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
        .map_err(DaoError::Database)?;

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
        .map_err(DaoError::Database)?;
        let Some(row) = inserted else {
            let existing = sqlx::query_as::<_, MessageRow>(
                "SELECT id,conversation_id,sender_user_id,recipient_user_id,kind,body,created_at,read_at FROM direct_messages WHERE sender_user_id = $1 AND client_message_id = $2",
            )
            .bind(&input.sender_user_id)
            .bind(&input.client_message_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            if existing.recipient_user_id != input.recipient_user_id
                || existing.body != input.body
                || parse_kind(&existing.kind)? != input.kind
            {
                return Err(DaoError::IdempotencyConflict);
            }
            enqueue_direct_message_notification(&mut transaction, &existing).await?;
            transaction.commit().await.map_err(DaoError::Database)?;
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
        .map_err(DaoError::Database)?;
        enqueue_direct_message_notification(&mut transaction, &row).await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        row.into_message()
    }

    async fn sender_restricted(&self, user_id: &str) -> Result<bool, DaoError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM direct_message_restrictions WHERE sender_user_id = $1)",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(DaoError::Database)
    }

    async fn list_conversations(
        &self,
        user_id: &str,
        cursor: Option<&ConversationCursor>,
        limit: usize,
    ) -> Result<Vec<pb::Conversation>, DaoError> {
        let rows = sqlx::query_as::<_, ConversationRow>(
            "SELECT c.id,CASE WHEN c.participant_one_id = $1 THEN c.participant_two_id ELSE c.participant_one_id END AS peer_user_id,LEFT(last_message.body,120) AS last_message_preview,c.last_message_at,COUNT(unread.id) AS unread_count FROM direct_conversations AS c JOIN direct_messages AS last_message ON last_message.id = c.last_message_id LEFT JOIN direct_messages AS unread ON unread.conversation_id = c.id AND unread.recipient_user_id = $1 AND unread.read_at IS NULL WHERE ($1 = c.participant_one_id OR $1 = c.participant_two_id) AND ($2::TIMESTAMPTZ IS NULL OR (c.last_message_at,c.id) < ($2,$3)) GROUP BY c.id,last_message.body,c.last_message_at ORDER BY c.last_message_at DESC,c.id DESC LIMIT $4",
        )
        .bind(user_id)
        .bind(cursor.map(|value| value.last_message_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
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
    ) -> Result<Vec<pb::DirectMessage>, DaoError> {
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
        .map_err(DaoError::Database)?;
        rows.reverse();
        rows.into_iter().map(MessageRow::into_message).collect()
    }

    async fn mark_read(
        &self,
        user_id: &str,
        conversation_id: &str,
        through_message_id: Option<&str>,
    ) -> Result<pb::MarkConversationReadResponse, DaoError> {
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
                .map_err(DaoError::Database)?
                .ok_or_else(|| DaoError::NotFound(message_id.to_string()))?,
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
        .map_err(DaoError::Database)?;
        Ok(pb::MarkConversationReadResponse {
            marked_count: result.rows_affected(),
            read_at: format_timestamp(read_at),
        })
    }

    async fn create_report(
        &self,
        input: CreateMessageReportInput,
    ) -> Result<pb::DirectMessageReport, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let message = select_message_for_report(&mut transaction, &input.message_id).await?;
        if message.recipient_user_id != input.reporter_user_id {
            return Err(DaoError::NotMessageRecipient);
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
            .map_err(DaoError::Database)?;
            if let Some(report_id) = inserted {
                let report = select_report(&mut transaction, &report_id).await?;
                transaction.commit().await.map_err(DaoError::Database)?;
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
                return Err(DaoError::ReportIdempotencyConflict);
            }
            transaction.commit().await.map_err(DaoError::Database)?;
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
        .map_err(DaoError::Database)?;
        let report = select_report(&mut transaction, &input.id).await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        report.into_report()
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::DirectMessageReport>, DaoError> {
        let rows = sqlx::query_as::<_, ReportRow>(
            "SELECT r.id AS report_id,r.reporter_user_id,r.reported_user_id,r.reason,r.details,r.status,r.reviewer_user_id,r.resolution,r.action,r.created_at,r.updated_at,m.id AS message_id,m.conversation_id,m.sender_user_id,m.recipient_user_id,m.kind,m.body,m.created_at AS message_created_at,m.read_at FROM direct_message_reports AS r JOIN direct_messages AS m ON m.id = r.message_id WHERE ($1::TEXT IS NULL OR r.status = $1) AND ($2::TIMESTAMPTZ IS NULL OR (r.created_at,r.id) > ($2,$3)) ORDER BY r.created_at ASC,r.id ASC LIMIT $4",
        )
        .bind(status.map(report_status_name).transpose()?)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter().map(ReportRow::into_report).collect()
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewMessageReportInput,
    ) -> Result<pb::DirectMessageReport, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let row = select_report_for_update(&mut transaction, report_id).await?;
        let mut report = row.into_report()?;
        let was_terminal = is_terminal_report(report.status);
        let reviewed = apply_report_review(&mut report, &input)?;
        if was_terminal {
            transaction.commit().await.map_err(DaoError::Database)?;
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
        .map_err(DaoError::Database)?;
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
            .map_err(DaoError::Database)?;
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        report.updated_at = format_timestamp(updated_at);
        Ok(report)
    }
}
