use super::*;

pub(crate) struct PostgresGrowthDao {
    pool: sqlx::PgPool,
}

impl PostgresGrowthDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GrowthDao for PostgresGrowthDao {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<pb::Journey>, DaoError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id = $1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(DaoError::Serialization))
            .collect()
    }

    async fn create_journey(
        &self,
        user_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Journey, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let request_payload = journey_idempotency_payload(&journey, &first_action);
        let inserted = sqlx::query(
            "INSERT INTO journeys (id, user_id, payload, status, progress, idempotency_key, idempotency_payload) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING",
        )
        .bind(&journey.id)
        .bind(user_id)
        .bind(serde_json::to_value(&journey).map_err(DaoError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .bind(idempotency_key.as_deref())
        .bind(&request_payload)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if inserted.rows_affected() == 0 {
            let Some(idempotency_key) = idempotency_key.as_deref() else {
                return Err(DaoError::IdempotencyConflict);
            };
            let (payload, stored_request_payload) = sqlx::query_as::<_, (serde_json::Value, Option<serde_json::Value>)>(
                "SELECT payload, idempotency_payload FROM journeys WHERE user_id=$1 AND idempotency_key=$2 FOR UPDATE",
            )
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or(DaoError::IdempotencyConflict)?;
            let existing: pb::Journey =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            return if stored_request_payload.as_ref() == Some(&request_payload) {
                transaction.commit().await.map_err(DaoError::Database)?;
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
            };
        }
        insert_postgres_action(&mut transaction, user_id, &first_action, None).await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(journey)
    }

    async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        journey: pb::Journey,
        actions: Vec<pb::Action>,
    ) -> Result<pb::Journey, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        // Keep route-derived Journey creation exactly-once under concurrent retries.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1 || chr(31) || $2, 0))")
            .bind(user_id)
            .bind(source_route_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        if let Some(payload) = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id = $1 AND source_route_id = $2",
        )
        .bind(user_id)
        .bind(source_route_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        {
            let existing: pb::Journey =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            upsert_postgres_route_intent(
                &mut transaction,
                user_id,
                source_route_id,
                true,
                Some(&existing.id),
            )
            .await?;
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(existing);
        }
        sqlx::query(
            "INSERT INTO journeys (id, user_id, source_route_id, payload, status, progress) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&journey.id)
        .bind(user_id)
        .bind(source_route_id)
        .bind(serde_json::to_value(&journey).map_err(DaoError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        for action in &actions {
            insert_postgres_action(&mut transaction, user_id, action, None).await?;
        }
        upsert_postgres_route_intent(
            &mut transaction,
            user_id,
            source_route_id,
            true,
            Some(&journey.id),
        )
        .await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(journey)
    }

    async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<pb::RouteParticipationIntent, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        if active && let Some(journey_id) = private_journey_id.as_deref() {
            let owned = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM journeys WHERE id = $1 AND user_id = $2)",
            )
            .bind(journey_id)
            .bind(user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            if !owned {
                return Err(DaoError::JourneyNotFound(journey_id.to_string()));
            }
        }
        let intent = upsert_postgres_route_intent(
            &mut transaction,
            user_id,
            route_id,
            active,
            active.then_some(private_journey_id.as_deref()).flatten(),
        )
        .await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(intent)
    }

    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<pb::JourneyDetail, DaoError> {
        let journey = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE id = $1 AND user_id = $2",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::JourneyNotFound(journey_id.to_string()))?;
        let journey = serde_json::from_value(journey).map_err(DaoError::Serialization)?;
        let actions = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE journey_id = $1 AND user_id = $2 ORDER BY created_at",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .into_iter()
        .map(|payload| serde_json::from_value(payload).map_err(DaoError::Serialization))
        .collect::<Result<Vec<pb::Action>, _>>()?;
        Ok(pb::JourneyDetail { journey, actions })
    }

    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, DaoError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::JourneyNotFound(journey_id.to_string()))?;
        let mut journey: pb::Journey =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        if let Some(title) = request.title {
            journey.title = title.trim().to_string();
        }
        if let Some(intent) = request.intent {
            journey.intent = intent.trim().to_string();
        }
        if let Some(duration_label) = request.duration_label {
            journey.duration_label = duration_label.trim().to_string();
        }
        if let Some(status) = request.status {
            journey.status = status;
            if status == pb::JourneyStatus::Completed as i32 {
                journey.progress = 100;
            }
        }
        sqlx::query(
            "UPDATE journeys SET payload = $1, status = $2, progress = $3, updated_at = now() WHERE id = $4 AND user_id = $5",
        )
        .bind(serde_json::to_value(&journey).map_err(DaoError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .bind(journey_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        Ok(journey)
    }

    async fn create_action(
        &self,
        user_id: &str,
        action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Action, DaoError> {
        let journey_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM journeys WHERE id = $1 AND user_id = $2)",
        )
        .bind(&action.journey_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        if !journey_exists {
            return Err(DaoError::JourneyNotFound(action.journey_id));
        }
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let inserted = insert_postgres_action(
            &mut transaction,
            user_id,
            &action,
            idempotency_key.as_deref(),
        )
        .await?;
        if !inserted {
            let Some(idempotency_key) = idempotency_key.as_deref() else {
                return Err(DaoError::IdempotencyConflict);
            };
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM actions WHERE user_id=$1 AND idempotency_key=$2 FOR UPDATE",
            )
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or(DaoError::IdempotencyConflict)?;
            let existing: pb::Action =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            return if same_action_content(&existing, &action) {
                transaction.commit().await.map_err(DaoError::Database)?;
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
            };
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(action)
    }

    async fn today(&self, user_id: &str, local_date: Date) -> Result<Vec<pb::Action>, DaoError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE user_id = $1 AND scheduled_for = $2 ORDER BY scheduled_at NULLS LAST, id",
        )
        .bind(user_id)
        .bind(local_date)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(DaoError::Serialization))
            .collect()
    }

    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<pb::Action, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let (payload, schedule_revision) = sqlx::query_as::<_, (serde_json::Value, i32)>(
            "SELECT payload, schedule_revision FROM actions WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(action_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::ActionNotFound(action_id.to_string()))?;
        let mut action: pb::Action =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        let successor = (action.state == pb::ActionState::Pending as i32)
            .then(|| recurring_successor(&action))
            .transpose()?
            .flatten();
        action.state = pb::ActionState::Completed as i32;
        sqlx::query(
            "UPDATE actions SET state = 'completed', payload = $1, updated_at = now() WHERE id = $2 AND user_id = $3",
        )
        .bind(serde_json::to_value(&action).map_err(DaoError::Serialization)?)
        .bind(action_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        sqlx::query(
            "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE action_id = $1 AND schedule_revision = $2 AND status IN ('queued', 'processing')",
        )
        .bind(action_id)
        .bind(schedule_revision)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if let Some(successor) = successor {
            insert_postgres_action(&mut transaction, user_id, &successor, None).await?;
        }
        refresh_postgres_journey(&mut transaction, user_id, action_id).await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(action)
    }

    async fn source_route_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, DaoError> {
        sqlx::query_scalar::<_, String>(
            "SELECT source_route_id FROM journeys WHERE id = $1 AND user_id = $2 AND source_route_id IS NOT NULL",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)
    }

    async fn source_knowledge_content_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, DaoError> {
        let sources = sqlx::query_scalar::<_, String>(
            "SELECT source_content_id FROM knowledge_resources WHERE user_id=$1 AND journey_id=$2 AND source_content_id IS NOT NULL GROUP BY source_content_id LIMIT 2",
        )
        .bind(user_id)
        .bind(journey_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        Ok((sources.len() == 1).then(|| sources[0].clone()))
    }

    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: pb::UpdateActionRequest,
    ) -> Result<pb::Action, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let (payload, stored_local_date, schedule_revision) = sqlx::query_as::<_, (serde_json::Value, Date, i32)>(
            "SELECT payload, scheduled_for, schedule_revision FROM actions WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(action_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::ActionNotFound(action_id.to_string()))?;
        let mut action: pb::Action =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        let stored_action = action.clone();
        let spawn_successor = request.state == Some(pb::ActionState::Skipped as i32)
            && stored_action.state == pb::ActionState::Pending as i32;
        if let Some(title) = request.title {
            action.title = title.trim().to_string();
        }
        if let Some(detail) = request.detail {
            action.detail = detail.trim().to_string();
        }
        if let Some(estimated_minutes) = request.estimated_minutes {
            action.estimated_minutes = estimated_minutes;
        }
        if let Some(scheduled_label) = request.scheduled_label {
            action.scheduled_label = scheduled_label.trim().to_string();
        }
        if let Some(scheduled_for) = request.scheduled_for {
            action.scheduled_for = Some(scheduled_for);
        }
        if let Some(scheduled_timezone) = request.scheduled_timezone {
            action.scheduled_timezone = Some(scheduled_timezone);
        }
        if let Some(state) = request.state {
            action.state = state;
        }
        let successor = spawn_successor
            .then(|| recurring_successor(&action))
            .transpose()?
            .flatten();
        let schedule_changed = action_schedule_changed(&stored_action, &action);
        let next_schedule_revision = schedule_revision + i32::from(schedule_changed);
        let schedule = postgres_action_schedule(&action, Some(stored_local_date))?;
        ensure_postgres_timezone(&mut transaction, schedule.timezone.as_deref()).await?;
        sqlx::query(
            "UPDATE actions SET payload = $1, state = $2, scheduled_for = $3, scheduled_at = $4, scheduled_timezone = $5, schedule_revision = $6, updated_at = now() WHERE id = $7 AND user_id = $8",
        )
        .bind(serde_json::to_value(&action).map_err(DaoError::Serialization)?)
        .bind(format_action_state(action.state))
        .bind(schedule.local_date)
        .bind(schedule.scheduled_at)
        .bind(schedule.timezone)
        .bind(next_schedule_revision)
        .bind(action_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if schedule_changed || action.state != pb::ActionState::Pending as i32 {
            sqlx::query(
                "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE action_id = $1 AND schedule_revision <= $2 AND status IN ('queued', 'processing')",
            )
            .bind(action_id)
            .bind(next_schedule_revision)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        }
        if let Some(successor) = successor {
            insert_postgres_action(&mut transaction, user_id, &successor, None).await?;
        }
        refresh_postgres_journey(&mut transaction, user_id, action_id).await?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(action)
    }

    async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::ReminderPreference, DaoError> {
        let row = sqlx::query_as::<_, (bool, i16, String, Option<time::Time>, Option<time::Time>, OffsetDateTime)>(
            "SELECT enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end, updated_at FROM reminder_preferences WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        row.map(reminder_preferences_from_row)
            .transpose()?
            .map_or_else(|| Ok(default_reminder_preferences()), Ok)
    }

    async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: pb::UpdateReminderPreferencesRequest,
    ) -> Result<pb::ReminderPreference, DaoError> {
        let preferences = reminder_preferences_from_request(request);
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        ensure_postgres_timezone(&mut transaction, Some(&preferences.timezone)).await?;
        let row = sqlx::query_as::<_, (bool, i16, String, Option<time::Time>, Option<time::Time>, OffsetDateTime)>(
            "INSERT INTO reminder_preferences (user_id, enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id) DO UPDATE SET enabled = EXCLUDED.enabled, lead_minutes = EXCLUDED.lead_minutes, timezone = EXCLUDED.timezone, quiet_hours_start = EXCLUDED.quiet_hours_start, quiet_hours_end = EXCLUDED.quiet_hours_end, version = reminder_preferences.version + 1, updated_at = now() RETURNING enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end, updated_at",
        )
        .bind(user_id)
        .bind(preferences.enabled)
        .bind(i16::try_from(preferences.lead_minutes).map_err(|_| {
            DaoError::Schedule("reminder lead minutes exceed database range".to_string())
        })?)
        .bind(&preferences.timezone)
        .bind(parse_quiet_time(preferences.quiet_hours_start.as_deref())?)
        .bind(parse_quiet_time(preferences.quiet_hours_end.as_deref())?)
        .fetch_one(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if !preferences.enabled {
            sqlx::query(
            "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE user_id = $1 AND status IN ('queued', 'processing')",
            )
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        reminder_preferences_from_row(row)
    }

    async fn register_push_device(
        &self,
        user_id: &str,
        request: pb::RegisterPushDeviceRequest,
    ) -> Result<pb::PushDevice, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let row = sqlx::query_as::<_, (String, String, bool, OffsetDateTime)>(
            "INSERT INTO push_devices (user_id, device_id, provider, endpoint, active, revoked_at) VALUES ($1, $2, $3, $4, true, NULL) ON CONFLICT (device_id) DO UPDATE SET user_id = EXCLUDED.user_id, provider = EXCLUDED.provider, endpoint = EXCLUDED.endpoint, active = true, revoked_at = NULL, updated_at = now() RETURNING device_id, provider, active, updated_at",
        )
        .bind(user_id)
        .bind(&request.device_id)
        .bind(format_push_provider(request.provider))
        .bind(&request.endpoint)
        .fetch_one(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        sqlx::query(
            "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE device_id = $1 AND user_id <> $2 AND status IN ('queued', 'processing')",
        )
        .bind(&request.device_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
        push_device_from_row(row)
    }

    async fn revoke_push_device(&self, user_id: &str, device_id: &str) -> Result<(), DaoError> {
        sqlx::query(
            "WITH revoked AS (UPDATE push_devices SET active = false, revoked_at = now(), updated_at = now() WHERE user_id = $1 AND device_id = $2 AND active RETURNING device_id) UPDATE reminder_deliveries d SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() FROM revoked r WHERE d.user_id = $1 AND d.device_id = r.device_id AND d.status IN ('queued', 'processing')",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        Ok(())
    }

    async fn list_notifications(
        &self,
        user_id: &str,
        request: pb::NotificationQueryRequest,
    ) -> Result<pb::NotificationPage, DaoError> {
        let limit = notification_limit(request.limit);
        let cursor = request
            .cursor
            .as_deref()
            .map(parse_notification_cursor)
            .transpose()?;
        let cursor_time = cursor
            .as_ref()
            .map(|(created_at, _)| OffsetDateTime::parse(created_at, &Rfc3339))
            .transpose()
            .map_err(|error| DaoError::Schedule(error.to_string()))?;
        let cursor_id = cursor
            .as_ref()
            .map(|(_, id)| Uuid::parse_str(id))
            .transpose()
            .map_err(|error| DaoError::Schedule(error.to_string()))?;
        let rows = sqlx::query_as::<_, (Uuid, String, String, String, String, serde_json::Value, Option<OffsetDateTime>, OffsetDateTime)>(
            "SELECT id, kind, source_id, title, body, data, read_at, created_at FROM user_notifications WHERE user_id = $1 AND ($2 = false OR read_at IS NULL) AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4::uuid)) ORDER BY created_at DESC, id DESC LIMIT $5",
        )
        .bind(user_id)
        .bind(request.unread_only.unwrap_or(false))
        .bind(cursor_time)
        .bind(cursor_id)
        .bind(i64::try_from(limit + 1).map_err(|_| {
            DaoError::Schedule("notification page size is invalid".to_string())
        })?)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        let mut items = rows
            .into_iter()
            .map(user_notification_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let unread_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_notifications WHERE user_id = $1 AND read_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        Ok(pb::NotificationPage {
            next_cursor: has_more
                .then(|| items.last().map(notification_cursor))
                .flatten(),
            items,
            unread_count: u32::try_from(unread_count).map_err(|_| {
                DaoError::Schedule("stored unread notification count is invalid".to_string())
            })?,
        })
    }

    async fn create_notification(
        &self,
        user_id: &str,
        request: pb::CreateNotificationRequest,
    ) -> Result<pb::UserNotification, DaoError> {
        let row = sqlx::query_as::<_, (Uuid, String, String, String, String, serde_json::Value, Option<OffsetDateTime>, OffsetDateTime)>(
            "INSERT INTO user_notifications (user_id, kind, source_id, title, body, data) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (kind, source_id) DO UPDATE SET source_id = EXCLUDED.source_id WHERE user_notifications.user_id = EXCLUDED.user_id RETURNING id, kind, source_id, title, body, data, read_at, created_at",
        )
        .bind(user_id)
        .bind(format_notification_kind(request.kind))
        .bind(&request.source_id)
        .bind(&request.title)
        .bind(&request.body)
        .bind(notification_data_to_json(&request.data))
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or(DaoError::NotificationSourceConflict(request.source_id))?;
        user_notification_from_row(row)
    }

    async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<pb::UserNotification, DaoError> {
        let notification_id = Uuid::parse_str(notification_id)
            .map_err(|_| DaoError::NotificationNotFound(notification_id.to_string()))?;
        let row = sqlx::query_as::<_, (Uuid, String, String, String, String, serde_json::Value, Option<OffsetDateTime>, OffsetDateTime)>(
            "UPDATE user_notifications SET read_at = COALESCE(read_at, now()) WHERE id = $1 AND user_id = $2 RETURNING id, kind, source_id, title, body, data, read_at, created_at",
        )
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::NotificationNotFound(notification_id.to_string()))?;
        user_notification_from_row(row)
    }

    async fn list_entries(&self, user_id: &str) -> Result<Vec<pb::GrowthEntry>, DaoError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM growth_entries WHERE user_id=$1 ORDER BY created_at DESC LIMIT 500",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(DaoError::Serialization))
            .collect()
    }

    async fn create_entry(
        &self,
        user_id: &str,
        entry: pb::GrowthEntry,
        idempotency_key: Option<String>,
    ) -> Result<pb::GrowthEntry, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let mut journey = None;
        if let Some(journey_id) = &entry.journey_id {
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM journeys WHERE id=$1 AND user_id=$2",
            )
            .bind(journey_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::EntryReferenceNotFound(journey_id.clone()))?;
            journey = Some(serde_json::from_value(payload).map_err(DaoError::Serialization)?);
        }
        if let Some(action_id) = &entry.action_id {
            let action_journey = sqlx::query_scalar::<_, String>(
                "SELECT journey_id FROM actions WHERE id=$1 AND user_id=$2",
            )
            .bind(action_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::EntryReferenceNotFound(action_id.clone()))?;
            if entry
                .journey_id
                .as_ref()
                .is_some_and(|journey_id| journey_id != &action_journey)
            {
                return Err(DaoError::EntryReferenceNotFound(action_id.clone()));
            }
        }
        let inserted = sqlx::query(
            "INSERT INTO growth_entries (id,user_id,journey_id,action_id,payload,published,created_at,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7::timestamptz,$8) ON CONFLICT DO NOTHING",
        )
        .bind(&entry.id)
        .bind(user_id)
        .bind(&entry.journey_id)
        .bind(&entry.action_id)
        .bind(serde_json::to_value(&entry).map_err(DaoError::Serialization)?)
        .bind(entry.published)
        .bind(&entry.created_at)
        .bind(idempotency_key.as_deref())
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if inserted.rows_affected() == 0 {
            let Some(idempotency_key) = idempotency_key.as_deref() else {
                return Err(DaoError::IdempotencyConflict);
            };
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM growth_entries WHERE user_id=$1 AND idempotency_key=$2 FOR UPDATE",
            )
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or(DaoError::IdempotencyConflict)?;
            let existing: pb::GrowthEntry =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            return if same_entry_content(&existing, &entry) {
                transaction.commit().await.map_err(DaoError::Database)?;
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
            };
        }
        if entry.publication_status == pb::EntryPublicationStatus::Pending as i32 {
            let payload = entry_publication_payload(user_id, &entry, journey.as_ref());
            sqlx::query(
                "INSERT INTO entry_publication_jobs (entry_id,user_id,payload) VALUES ($1,$2,$3)",
            )
            .bind(&entry.id)
            .bind(user_id)
            .bind(payload)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(entry)
    }

    async fn retry_entry_publication(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<pb::GrowthEntry, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM growth_entries WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(entry_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::EntryNotFound(entry_id.to_string()))?;
        let mut entry: pb::GrowthEntry =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        if entry.publication_status != pb::EntryPublicationStatus::Failed as i32 {
            return Err(DaoError::EntryPublicationNotRetryable);
        }
        entry.publication_status = pb::EntryPublicationStatus::Pending as i32;
        entry.publication_error = None;
        entry.published = false;
        sqlx::query(
            "UPDATE growth_entries SET payload=$3,published=false WHERE id=$1 AND user_id=$2",
        )
        .bind(entry_id)
        .bind(user_id)
        .bind(serde_json::to_value(&entry).map_err(DaoError::Serialization)?)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        let requeued = sqlx::query(
            "UPDATE entry_publication_jobs SET status='pending',attempts=0,available_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE entry_id=$1 AND user_id=$2 AND status='dead'",
        )
        .bind(entry_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if requeued.rows_affected() != 1 {
            return Err(DaoError::EntryPublicationNotRetryable);
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(entry)
    }

    async fn review_snapshot(&self, user_id: &str) -> Result<ReviewSnapshot, DaoError> {
        let journey_rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id=$1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        let action_rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE user_id=$1 AND updated_at >= date_trunc('week',now())",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        let entry_rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM growth_entries WHERE user_id=$1 AND created_at >= date_trunc('week',now()) ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        Ok(ReviewSnapshot {
            journeys: deserialize_rows(journey_rows)?,
            actions: deserialize_rows(action_rows)?,
            entries: deserialize_rows(entry_rows)?,
        })
    }

    async fn save_weekly_review(
        &self,
        user_id: &str,
        review: pb::ReviewRecord,
    ) -> Result<pb::ReviewRecord, DaoError> {
        let summary = review
            .summary
            .as_ref()
            .ok_or(DaoError::InvalidWeeklyReview)?;
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let inserted = sqlx::query(
            "INSERT INTO weekly_reviews (id,user_id,period_start,period_end,payload,created_at,updated_at) VALUES ($1,$2,$3::date,$4::date,$5,$6::timestamptz,$7::timestamptz) ON CONFLICT (user_id,period_start,period_end) DO NOTHING",
        )
        .bind(&review.id)
        .bind(user_id)
        .bind(&summary.period_start)
        .bind(&summary.period_end)
        .bind(serde_json::to_value(&review).map_err(DaoError::Serialization)?)
        .bind(&review.created_at)
        .bind(&review.updated_at)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if inserted.rows_affected() == 1 {
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(review);
        }
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM weekly_reviews WHERE user_id=$1 AND period_start=$2::date AND period_end=$3::date FOR UPDATE",
        )
        .bind(user_id)
        .bind(&summary.period_start)
        .bind(&summary.period_end)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or(DaoError::InvalidWeeklyReview)?;
        let mut existing: pb::ReviewRecord =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        // The first confirmed review is the immutable period snapshot. A
        // later PUT only edits the user's interpretation and next focus.
        existing.reflection = review.reflection;
        existing.next_focus = review.next_focus;
        existing.updated_at = review.updated_at;
        sqlx::query(
            "UPDATE weekly_reviews SET payload=$1, updated_at=$2::timestamptz WHERE id=$3 AND user_id=$4",
        )
        .bind(serde_json::to_value(&existing).map_err(DaoError::Serialization)?)
        .bind(&existing.updated_at)
        .bind(&existing.id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(existing)
    }

    async fn apply_weekly_review_adjustment(
        &self,
        user_id: &str,
        review_id: &str,
        suggestion_index: u32,
    ) -> Result<AppliedReviewAdjustment, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM weekly_reviews WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(review_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::ReviewNotFound(review_id.to_string()))?;
        let mut review: pb::ReviewRecord =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        if let Some(decision) = review
            .applied_adjustments
            .iter()
            .find(|decision| decision.suggestion_index == suggestion_index)
            .cloned()
        {
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(AppliedReviewAdjustment { review, decision });
        }
        let suggestion = review
            .summary
            .as_ref()
            .ok_or(DaoError::InvalidWeeklyReview)?
            .adjustment_suggestions
            .get(
                usize::try_from(suggestion_index)
                    .map_err(|_| DaoError::ReviewAdjustmentNotFound(suggestion_index))?,
            )
            .cloned()
            .ok_or(DaoError::ReviewAdjustmentNotFound(suggestion_index))?;
        let decision = if let Some(patch) = suggestion.action_patch {
            let expected_minutes = patch
                .expected_estimated_minutes
                .ok_or(DaoError::ReviewAdjustmentStale)?;
            let proposed_minutes = patch
                .estimated_minutes
                .ok_or(DaoError::ReviewAdjustmentStale)?;
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM actions WHERE id=$1 AND user_id=$2 FOR UPDATE",
            )
            .bind(&patch.action_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::ActionNotFound(patch.action_id.clone()))?;
            let mut action: pb::Action =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            if action.state != pb::ActionState::Pending as i32
                || action.estimated_minutes != expected_minutes
            {
                return Err(DaoError::ReviewAdjustmentStale);
            }
            action.estimated_minutes = proposed_minutes;
            sqlx::query(
                "UPDATE actions SET payload=$1, updated_at=now() WHERE id=$2 AND user_id=$3",
            )
            .bind(serde_json::to_value(&action).map_err(DaoError::Serialization)?)
            .bind(&patch.action_id)
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            pb::ReviewAdjustmentDecision {
                suggestion_index,
                applied_at: current_timestamp(),
                action: Some(action),
                journey: None,
            }
        } else if let Some(patch) = suggestion.journey_patch {
            let expected_status = patch
                .expected_status
                .ok_or(DaoError::ReviewAdjustmentStale)?;
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM journeys WHERE id=$1 AND user_id=$2 FOR UPDATE",
            )
            .bind(&patch.journey_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::JourneyNotFound(patch.journey_id.clone()))?;
            let mut journey: pb::Journey =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            if journey.status != expected_status {
                return Err(DaoError::ReviewAdjustmentStale);
            }
            journey.status = patch.status;
            sqlx::query(
                "UPDATE journeys SET payload=$1, status=$2, updated_at=now() WHERE id=$3 AND user_id=$4",
            )
            .bind(serde_json::to_value(&journey).map_err(DaoError::Serialization)?)
            .bind(format_status(journey.status))
            .bind(&patch.journey_id)
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            pb::ReviewAdjustmentDecision {
                suggestion_index,
                applied_at: current_timestamp(),
                action: None,
                journey: Some(journey),
            }
        } else {
            return Err(DaoError::ReviewAdjustmentStale);
        };
        review.applied_adjustments.push(decision.clone());
        review.updated_at = decision.applied_at.clone();
        sqlx::query(
            "UPDATE weekly_reviews SET payload=$1, updated_at=$2::timestamptz WHERE id=$3 AND user_id=$4",
        )
        .bind(serde_json::to_value(&review).map_err(DaoError::Serialization)?)
        .bind(&review.updated_at)
        .bind(review_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(AppliedReviewAdjustment { review, decision })
    }

    async fn list_knowledge(
        &self,
        user_id: &str,
        query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, DaoError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM knowledge_resources WHERE user_id=$1 AND ($2::text IS NULL OR payload->>'title' ILIKE '%' || $2 || '%' OR payload->>'creator' ILIKE '%' || $2 || '%' OR payload->>'summary' ILIKE '%' || $2 || '%' OR payload->>'body' ILIKE '%' || $2 || '%') AND ($3::text IS NULL OR kind=$3) AND ($4::text IS NULL OR status=$4) AND ($5::text IS NULL OR tags @> ARRAY[$5]::text[]) ORDER BY updated_at DESC LIMIT 500",
        )
        .bind(user_id)
        .bind(query.q)
        .bind(query.kind.map(format_knowledge_kind))
        .bind(query.status.map(format_knowledge_status))
        .bind(query.tag)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        deserialize_rows(rows)
    }

    async fn create_knowledge(
        &self,
        user_id: &str,
        resource: pb::KnowledgeResource,
        idempotency_key: Option<String>,
    ) -> Result<pb::KnowledgeResource, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(journey_id) = resource.journey_id.as_deref() {
            ensure_postgres_journey(&mut transaction, user_id, journey_id).await?;
        }
        let result = sqlx::query(
            "INSERT INTO knowledge_resources (id,user_id,kind,status,title,tags,journey_id,idempotency_key,source_content_id,payload,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::timestamptz,$12::timestamptz) ON CONFLICT DO NOTHING",
        )
        .bind(&resource.id)
        .bind(user_id)
        .bind(format_knowledge_kind(resource.kind))
        .bind(format_knowledge_status(resource.status))
        .bind(&resource.title)
        .bind(&resource.tags)
        .bind(&resource.journey_id)
        .bind(&idempotency_key)
        .bind(&resource.source_content_id)
        .bind(serde_json::to_value(&resource).map_err(DaoError::Serialization)?)
        .bind(&resource.created_at)
        .bind(&resource.updated_at)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        if result.rows_affected() == 0 {
            let payload = if let Some(source_content_id) = resource.source_content_id.as_deref() {
                sqlx::query_scalar::<_, serde_json::Value>(
                    "SELECT payload FROM knowledge_resources WHERE user_id=$1 AND source_content_id=$2",
                )
                .bind(user_id)
                .bind(source_content_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(DaoError::Database)?
            } else if let Some(idempotency_key) = idempotency_key.as_deref() {
                sqlx::query_scalar::<_, serde_json::Value>(
                    "SELECT payload FROM knowledge_resources WHERE user_id=$1 AND idempotency_key=$2",
                )
                .bind(user_id)
                .bind(idempotency_key)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(DaoError::Database)?
            } else {
                None
            }
            .ok_or(DaoError::IdempotencyConflict)?;
            let existing: pb::KnowledgeResource =
                serde_json::from_value(payload).map_err(DaoError::Serialization)?;
            transaction.commit().await.map_err(DaoError::Database)?;
            return if resource.source_content_id.is_some()
                || same_knowledge_content(&existing, &resource)
            {
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
            };
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(resource)
    }

    async fn start_knowledge_journey(
        &self,
        user_id: &str,
        resource_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
    ) -> Result<pb::KnowledgeJourney, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        // Lock the resource first. It is the per-user, per-resource idempotency
        // boundary, so concurrent retries cannot create two private Journeys.
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM knowledge_resources WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(resource_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::KnowledgeNotFound(resource_id.to_string()))?;
        let mut resource: pb::KnowledgeResource =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        if let Some(existing_journey_id) = resource.journey_id.as_deref() {
            let journey = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM journeys WHERE id=$1 AND user_id=$2",
            )
            .bind(existing_journey_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::JourneyNotFound(existing_journey_id.to_string()))?;
            let journey = serde_json::from_value(journey).map_err(DaoError::Serialization)?;
            let actions = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM actions WHERE journey_id=$1 AND user_id=$2 ORDER BY created_at",
            )
            .bind(existing_journey_id)
            .bind(user_id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
            .into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(DaoError::Serialization))
            .collect::<Result<Vec<pb::Action>, _>>()?;
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(pb::KnowledgeJourney {
                resource: Some(resource),
                journey: Some(pb::JourneyDetail {
                    journey: Some(journey),
                    actions,
                }),
            });
        }

        sqlx::query(
            "INSERT INTO journeys (id,user_id,payload,status,progress) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(&journey.id)
        .bind(user_id)
        .bind(serde_json::to_value(&journey).map_err(DaoError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        insert_postgres_action(&mut transaction, user_id, &first_action, None).await?;

        resource.journey_id = Some(journey.id.clone());
        resource.status = pb::KnowledgeResourceStatus::Active as i32;
        resource.updated_at = current_timestamp();
        sqlx::query(
            "UPDATE knowledge_resources SET status=$1,journey_id=$2,payload=$3,updated_at=$4::timestamptz WHERE id=$5 AND user_id=$6",
        )
        .bind(format_knowledge_status(resource.status))
        .bind(&resource.journey_id)
        .bind(serde_json::to_value(&resource).map_err(DaoError::Serialization)?)
        .bind(&resource.updated_at)
        .bind(resource_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(pb::KnowledgeJourney {
            resource: Some(resource),
            journey: Some(pb::JourneyDetail {
                journey: Some(journey),
                actions: vec![first_action],
            }),
        })
    }

    async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        request: pb::UpdateKnowledgeRequest,
    ) -> Result<pb::KnowledgeResource, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(journey_id) = request.journey_id.as_deref() {
            ensure_postgres_journey(&mut transaction, user_id, journey_id).await?;
        }
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM knowledge_resources WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(resource_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::KnowledgeNotFound(resource_id.to_string()))?;
        let mut resource: pb::KnowledgeResource =
            serde_json::from_value(payload).map_err(DaoError::Serialization)?;
        apply_knowledge_update(&mut resource, request);
        sqlx::query(
            "UPDATE knowledge_resources SET kind=$1,status=$2,title=$3,tags=$4,journey_id=$5,payload=$6,updated_at=$7::timestamptz WHERE id=$8 AND user_id=$9",
        )
        .bind(format_knowledge_kind(resource.kind))
        .bind(format_knowledge_status(resource.status))
        .bind(&resource.title)
        .bind(&resource.tags)
        .bind(&resource.journey_id)
        .bind(serde_json::to_value(&resource).map_err(DaoError::Serialization)?)
        .bind(&resource.updated_at)
        .bind(resource_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(resource)
    }
}
