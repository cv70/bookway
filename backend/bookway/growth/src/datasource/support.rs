use std::collections::HashMap;

use crate::api::pb;
use async_trait::async_trait;
use thiserror::Error;
use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("journey {0} was not found")]
    JourneyNotFound(String),
    #[error("action {0} was not found")]
    ActionNotFound(String),
    #[error("notification {0} was not found")]
    NotificationNotFound(String),
    #[error("notification source {0} belongs to a different user")]
    NotificationSourceConflict(String),
    #[error("entry reference {0} was not found or does not belong to this user")]
    EntryReferenceNotFound(String),
    #[error("entry {0} was not found")]
    EntryNotFound(String),
    #[error("entry publication can only be retried after a terminal delivery failure")]
    EntryPublicationNotRetryable,
    #[error("knowledge resource {0} was not found")]
    KnowledgeNotFound(String),
    #[error("knowledge resource reference {0} was not found or does not belong to this user")]
    KnowledgeReferenceNotFound(String),
    #[error("weekly review payload is missing its summary")]
    InvalidWeeklyReview,
    #[error("weekly review {0} was not found")]
    ReviewNotFound(String),
    #[error("weekly review adjustment {0} was not found")]
    ReviewAdjustmentNotFound(u32),
    #[error("weekly review adjustment is no longer applicable")]
    ReviewAdjustmentStale,
    #[error("idempotency key was already used with different content")]
    IdempotencyConflict,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored growth data is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
    #[error("stored action schedule is invalid: {0}")]
    Schedule(String),
}

#[async_trait]
pub(crate) trait GrowthDao: Send + Sync {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<pb::Journey>, DaoError>;
    async fn create_journey(
        &self,
        user_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Journey, DaoError>;
    async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        journey: pb::Journey,
        actions: Vec<pb::Action>,
    ) -> Result<pb::Journey, DaoError>;
    async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<pb::RouteParticipationIntent, DaoError>;
    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<pb::JourneyDetail, DaoError>;
    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, DaoError>;
    async fn create_action(
        &self,
        user_id: &str,
        action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Action, DaoError>;
    async fn today(&self, user_id: &str, local_date: Date) -> Result<Vec<pb::Action>, DaoError>;
    async fn complete_action(&self, user_id: &str, action_id: &str)
    -> Result<pb::Action, DaoError>;
    async fn source_route_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, DaoError>;
    async fn source_knowledge_content_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, DaoError>;
    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: pb::UpdateActionRequest,
    ) -> Result<pb::Action, DaoError>;
    async fn reminder_preferences(&self, user_id: &str)
    -> Result<pb::ReminderPreference, DaoError>;
    async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: pb::UpdateReminderPreferencesRequest,
    ) -> Result<pb::ReminderPreference, DaoError>;
    async fn register_push_device(
        &self,
        user_id: &str,
        request: pb::RegisterPushDeviceRequest,
    ) -> Result<pb::PushDevice, DaoError>;
    async fn revoke_push_device(&self, user_id: &str, device_id: &str) -> Result<(), DaoError>;
    async fn list_notifications(
        &self,
        user_id: &str,
        request: pb::NotificationQueryRequest,
    ) -> Result<pb::NotificationPage, DaoError>;
    async fn create_notification(
        &self,
        user_id: &str,
        request: pb::CreateNotificationRequest,
    ) -> Result<pb::UserNotification, DaoError>;
    async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<pb::UserNotification, DaoError>;
    async fn list_entries(&self, user_id: &str) -> Result<Vec<pb::GrowthEntry>, DaoError>;
    async fn create_entry(
        &self,
        user_id: &str,
        entry: pb::GrowthEntry,
        idempotency_key: Option<String>,
    ) -> Result<pb::GrowthEntry, DaoError>;
    async fn retry_entry_publication(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<pb::GrowthEntry, DaoError>;
    async fn review_snapshot(&self, user_id: &str) -> Result<ReviewSnapshot, DaoError>;
    async fn save_weekly_review(
        &self,
        user_id: &str,
        review: pb::ReviewRecord,
    ) -> Result<pb::ReviewRecord, DaoError>;
    async fn apply_weekly_review_adjustment(
        &self,
        user_id: &str,
        review_id: &str,
        suggestion_index: u32,
    ) -> Result<AppliedReviewAdjustment, DaoError>;
    async fn list_knowledge(
        &self,
        user_id: &str,
        query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, DaoError>;
    async fn create_knowledge(
        &self,
        user_id: &str,
        resource: pb::KnowledgeResource,
        idempotency_key: Option<String>,
    ) -> Result<pb::KnowledgeResource, DaoError>;
    async fn start_knowledge_journey(
        &self,
        user_id: &str,
        resource_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
    ) -> Result<pb::KnowledgeJourney, DaoError>;
    async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        request: pb::UpdateKnowledgeRequest,
    ) -> Result<pb::KnowledgeResource, DaoError>;
}

pub(crate) struct ReviewSnapshot {
    pub(crate) journeys: Vec<pb::Journey>,
    pub(crate) actions: Vec<pb::Action>,
    pub(crate) entries: Vec<pb::GrowthEntry>,
}

pub(crate) struct AppliedReviewAdjustment {
    pub(crate) review: pb::ReviewRecord,
    pub(crate) decision: pb::ReviewAdjustmentDecision,
}

struct State {
    journeys: Vec<pb::Journey>,
    actions: HashMap<String, pb::Action>,
    journey_owners: HashMap<String, String>,
    journey_idempotency: HashMap<(String, String), (String, serde_json::Value)>,
    route_journeys: HashMap<(String, String), String>,
    route_participation_intents: HashMap<(String, String), pb::RouteParticipationIntent>,
    action_owners: HashMap<String, String>,
    action_idempotency: HashMap<(String, String), String>,
    entries: Vec<pb::GrowthEntry>,
    entry_owners: HashMap<String, String>,
    entry_idempotency: HashMap<(String, String), String>,
    knowledge_resources: HashMap<String, pb::KnowledgeResource>,
    knowledge_owners: HashMap<String, String>,
    knowledge_idempotency: HashMap<(String, String), String>,
    knowledge_sources: HashMap<(String, String), String>,
    reminder_preferences: HashMap<String, pb::ReminderPreference>,
    push_devices: HashMap<String, (String, pb::PushDevice)>,
    notifications: Vec<pb::UserNotification>,
    notification_owners: HashMap<String, String>,
    weekly_reviews: HashMap<(String, String, String), pb::ReviewRecord>,
}

fn memory_journey_detail(
    state: &State,
    user_id: &str,
    journey_id: &str,
) -> Result<pb::JourneyDetail, DaoError> {
    let journey = state
        .journeys
        .iter()
        .find(|journey| {
            journey.id == journey_id
                && state
                    .journey_owners
                    .get(&journey.id)
                    .is_some_and(|owner| owner == user_id)
        })
        .cloned()
        .ok_or_else(|| DaoError::JourneyNotFound(journey_id.to_string()))?;
    let mut actions: Vec<_> = state
        .actions
        .values()
        .filter(|action| action.journey_id == journey_id)
        .cloned()
        .collect();
    actions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(pb::JourneyDetail {
        journey: Some(journey),
        actions,
    })
}

fn default_reminder_preferences() -> pb::ReminderPreference {
    pb::ReminderPreference {
        enabled: false,
        lead_minutes: 0,
        timezone: "UTC".to_string(),
        quiet_hours_start: None,
        quiet_hours_end: None,
        updated_at: now_rfc3339(),
    }
}

fn reminder_preferences_from_request(
    request: pb::UpdateReminderPreferencesRequest,
) -> pb::ReminderPreference {
    pb::ReminderPreference {
        enabled: request.enabled,
        lead_minutes: request.lead_minutes,
        timezone: request.timezone.trim().to_string(),
        quiet_hours_start: request
            .quiet_hours_start
            .map(|value| value.trim().to_string()),
        quiet_hours_end: request
            .quiet_hours_end
            .map(|value| value.trim().to_string()),
        updated_at: now_rfc3339(),
    }
}

fn reminder_preferences_from_row(
    (enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end, updated_at): (
        bool,
        i16,
        String,
        Option<time::Time>,
        Option<time::Time>,
        OffsetDateTime,
    ),
) -> Result<pb::ReminderPreference, DaoError> {
    Ok(pb::ReminderPreference {
        enabled,
        lead_minutes: u32::try_from(lead_minutes).map_err(|_| {
            DaoError::Schedule("stored reminder lead minutes is invalid".to_string())
        })?,
        timezone,
        quiet_hours_start: quiet_hours_start.map(format_quiet_time).transpose()?,
        quiet_hours_end: quiet_hours_end.map(format_quiet_time).transpose()?,
        updated_at: updated_at
            .format(&Rfc3339)
            .map_err(|error| DaoError::Schedule(error.to_string()))?,
    })
}

fn parse_quiet_time(value: Option<&str>) -> Result<Option<time::Time>, DaoError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let format = time::format_description::parse_borrowed::<2>("[hour padding:zero]:[minute]")
        .map_err(|error| DaoError::Schedule(error.to_string()))?;
    time::Time::parse(value, &format)
        .map(Some)
        .map_err(|error| DaoError::Schedule(error.to_string()))
}

fn format_quiet_time(value: time::Time) -> Result<String, DaoError> {
    let format = time::format_description::parse_borrowed::<2>("[hour padding:zero]:[minute]")
        .map_err(|error| DaoError::Schedule(error.to_string()))?;
    value
        .format(&format)
        .map_err(|error| DaoError::Schedule(error.to_string()))
}

fn format_push_provider(provider: i32) -> &'static str {
    match pb::PushProvider::try_from(provider).unwrap_or(pb::PushProvider::Expo) {
        pb::PushProvider::Expo => "expo",
        pb::PushProvider::Fcm => "fcm",
        pb::PushProvider::Apns => "apns",
    }
}

fn format_notification_kind(kind: i32) -> &'static str {
    match pb::NotificationKind::try_from(kind).unwrap_or(pb::NotificationKind::ActionReminder) {
        pb::NotificationKind::ActionReminder => "action_reminder",
        pb::NotificationKind::Community => "community",
        pb::NotificationKind::System => "system",
    }
}

fn push_device_from_row(
    (device_id, provider, active, updated_at): (String, String, bool, OffsetDateTime),
) -> Result<pb::PushDevice, DaoError> {
    let provider = match provider.as_str() {
        "expo" => pb::PushProvider::Expo,
        "fcm" => pb::PushProvider::Fcm,
        "apns" => pb::PushProvider::Apns,
        _ => {
            return Err(DaoError::Schedule(
                "stored push provider is invalid".to_string(),
            ));
        }
    };
    Ok(pb::PushDevice {
        device_id,
        provider: provider as i32,
        active,
        updated_at: updated_at
            .format(&Rfc3339)
            .map_err(|error| DaoError::Schedule(error.to_string()))?,
    })
}

fn notification_limit(value: Option<u32>) -> usize {
    usize::try_from(value.unwrap_or(30).clamp(1, 100)).unwrap_or(100)
}

fn parse_notification_cursor(value: &str) -> Result<(String, String), DaoError> {
    let (created_at, id) = value
        .split_once('|')
        .ok_or_else(|| DaoError::Schedule("notification cursor is invalid".to_string()))?;
    OffsetDateTime::parse(created_at, &Rfc3339)
        .map_err(|_| DaoError::Schedule("notification cursor is invalid".to_string()))?;
    Uuid::parse_str(id)
        .map_err(|_| DaoError::Schedule("notification cursor is invalid".to_string()))?;
    Ok((created_at.to_string(), id.to_string()))
}

fn notification_cursor(notification: &pb::UserNotification) -> String {
    format!("{}|{}", notification.created_at, notification.id)
}

fn user_notification_from_row(
    (id, kind, source_id, title, body, data, read_at, created_at): (
        Uuid,
        String,
        String,
        String,
        String,
        serde_json::Value,
        Option<OffsetDateTime>,
        OffsetDateTime,
    ),
) -> Result<pb::UserNotification, DaoError> {
    let kind = match kind.as_str() {
        "action_reminder" => pb::NotificationKind::ActionReminder,
        "community" => pb::NotificationKind::Community,
        "system" => pb::NotificationKind::System,
        _ => {
            return Err(DaoError::Schedule(
                "stored notification kind is invalid".to_string(),
            ));
        }
    };
    Ok(pb::UserNotification {
        id: id.to_string(),
        kind: kind as i32,
        source_id,
        title,
        body,
        data: notification_data_from_json(data)?,
        read_at: read_at
            .map(|value| value.format(&Rfc3339))
            .transpose()
            .map_err(|error| DaoError::Schedule(error.to_string()))?,
        created_at: created_at
            .format(&Rfc3339)
            .map_err(|error| DaoError::Schedule(error.to_string()))?,
    })
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn entry_publication_payload(
    user_id: &str,
    entry: &pb::GrowthEntry,
    journey: Option<&pb::Journey>,
) -> serde_json::Value {
    let title = public_text_preview(&entry.body, 48);
    let summary = public_text_preview(&entry.body, 144);
    serde_json::json!({
        "user_id": user_id,
        "idempotency_key": format!("entry-publication:{}", entry.id),
        "title": title,
        "summary": summary,
        "body": entry.body.clone(),
        "domain": journey.map(|journey| journey.domain).unwrap_or(pb::GrowthDomain::Leisure as i32),
        "media_asset_ids": entry.photo_media_id.iter().cloned().collect::<Vec<_>>(),
    })
}

fn public_text_preview(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = normalized.chars().take(limit).collect::<String>();
    if normalized.chars().count() > limit {
        preview.push_str("...");
    }
    preview
}

fn refresh_memory_journey(state: &mut State, journey_id: &str) {
    let total = state
        .actions
        .values()
        .filter(|action| action.journey_id == journey_id)
        .count();
    let completed = state
        .actions
        .values()
        .filter(|action| {
            action.journey_id == journey_id && action.state == pb::ActionState::Completed as i32
        })
        .count();
    let next_action = state
        .actions
        .values()
        .filter(|action| {
            action.journey_id == journey_id && action.state == pb::ActionState::Pending as i32
        })
        .min_by(|left, right| left.id.cmp(&right.id))
        .map(|action| action.title.clone())
        .unwrap_or_else(|| "路线已完成".to_string());
    if let Some(journey) = state
        .journeys
        .iter_mut()
        .find(|journey| journey.id == journey_id)
    {
        if journey.journey_type != crate::api::pb::JourneyType::Habit as i32 {
            journey.progress = u32::try_from(
                completed
                    .saturating_mul(100)
                    .checked_div(total)
                    .unwrap_or_default(),
            )
            .unwrap_or(u32::MAX);
        }
        journey.next_action = next_action;
    }
}

fn recurring_successor(action: &pb::Action) -> Result<Option<pb::Action>, DaoError> {
    let Some(recurrence) = action.recurrence.as_ref() else {
        return Ok(None);
    };
    let (Some(scheduled_for), Some(_scheduled_timezone)) = (
        action.scheduled_for.as_deref(),
        action.scheduled_timezone.as_deref(),
    ) else {
        return Err(DaoError::Schedule(
            "a recurring action must have a timestamp and timezone".to_string(),
        ));
    };
    let scheduled_at = OffsetDateTime::parse(scheduled_for, &Rfc3339)
        .map_err(|error| DaoError::Schedule(error.to_string()))?;
    let anchor_date = recurrence
        .anchor_date
        .as_deref()
        .ok_or_else(|| DaoError::Schedule("recurrence anchor date is missing".to_string()))
        .and_then(dao_local_date)?;
    let end_date = recurrence
        .ends_on
        .as_deref()
        .map(dao_local_date)
        .transpose()?;
    let next_date = match pb::ActionRecurrenceFrequency::try_from(recurrence.frequency) {
        Ok(pb::ActionRecurrenceFrequency::Daily) => {
            scheduled_at.date() + time::Duration::days(i64::from(recurrence.interval))
        }
        Ok(pb::ActionRecurrenceFrequency::Weekly) => next_weekly_recurrence_date(
            scheduled_at.date(),
            anchor_date,
            recurrence.interval,
            &recurrence.weekdays,
        )?,
        Err(_) => {
            return Err(DaoError::Schedule(
                "stored recurrence frequency is invalid".to_string(),
            ));
        }
    };
    if end_date.is_some_and(|end_date| next_date > end_date) {
        return Ok(None);
    }
    let offset_days = (next_date - scheduled_at.date()).whole_days();
    let next_scheduled_at = scheduled_at + time::Duration::days(offset_days);
    let scheduled_for = next_scheduled_at
        .format(&Rfc3339)
        .map_err(|error| DaoError::Schedule(error.to_string()))?;
    let mut successor = action.clone();
    successor.id = Uuid::now_v7().to_string();
    successor.scheduled_for = Some(scheduled_for);
    successor.scheduled_label = format!("{} {}", next_date, next_scheduled_at.time());
    successor.state = pb::ActionState::Pending as i32;
    Ok(Some(successor))
}

fn next_weekly_recurrence_date(
    current_date: Date,
    anchor_date: Date,
    interval: u32,
    weekdays: &[i32],
) -> Result<Date, DaoError> {
    if weekdays.is_empty() {
        return Err(DaoError::Schedule(
            "a weekly recurrence must include weekdays".to_string(),
        ));
    }
    let anchor_week_start = anchor_date
        - time::Duration::days(i64::from(anchor_date.weekday().number_days_from_monday()));
    let mut candidate = current_date + time::Duration::days(1);
    // At most `interval` weeks plus one week need to be examined to find the
    // next matching local weekday.
    for _ in 0..=(usize::try_from(interval).unwrap_or(365) * 7) {
        let candidate_week_start = candidate
            - time::Duration::days(i64::from(candidate.weekday().number_days_from_monday()));
        let weeks_since_anchor = (candidate_week_start - anchor_week_start).whole_days() / 7;
        if weeks_since_anchor >= 0
            && weeks_since_anchor % i64::from(interval) == 0
            && weekdays
                .iter()
                .any(|weekday| weekday_matches(*weekday, candidate.weekday()))
        {
            return Ok(candidate);
        }
        candidate += time::Duration::days(1);
    }
    Err(DaoError::Schedule(
        "could not calculate the next recurring occurrence".to_string(),
    ))
}

fn weekday_matches(expected: i32, actual: time::Weekday) -> bool {
    matches!(
        (pb::Weekday::try_from(expected), actual),
        (Ok(pb::Weekday::Monday), time::Weekday::Monday)
            | (Ok(pb::Weekday::Tuesday), time::Weekday::Tuesday)
            | (Ok(pb::Weekday::Wednesday), time::Weekday::Wednesday)
            | (Ok(pb::Weekday::Thursday), time::Weekday::Thursday)
            | (Ok(pb::Weekday::Friday), time::Weekday::Friday)
            | (Ok(pb::Weekday::Saturday), time::Weekday::Saturday)
            | (Ok(pb::Weekday::Sunday), time::Weekday::Sunday)
    )
}

fn dao_local_date(value: &str) -> Result<Date, DaoError> {
    let format = time::format_description::parse_borrowed::<3>("[year]-[month]-[day]")
        .map_err(|error| DaoError::Schedule(error.to_string()))?;
    Date::parse(value, &format).map_err(|error| DaoError::Schedule(error.to_string()))
}

fn action_schedule_changed(stored: &pb::Action, updated: &pb::Action) -> bool {
    stored.scheduled_for != updated.scheduled_for
        || stored.scheduled_timezone != updated.scheduled_timezone
}

struct PostgresActionSchedule {
    local_date: Date,
    scheduled_at: Option<OffsetDateTime>,
    timezone: Option<String>,
}

async fn insert_postgres_action(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    action: &pb::Action,
    idempotency_key: Option<&str>,
) -> Result<bool, DaoError> {
    let schedule = postgres_action_schedule(action, None)?;
    ensure_postgres_timezone(transaction, schedule.timezone.as_deref()).await?;
    let result = sqlx::query(
        "INSERT INTO actions (id, journey_id, user_id, payload, state, scheduled_for, scheduled_at, scheduled_timezone, idempotency_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT DO NOTHING",
    )
    .bind(&action.id)
    .bind(&action.journey_id)
    .bind(user_id)
    .bind(serde_json::to_value(action).map_err(DaoError::Serialization)?)
    .bind(format_action_state(action.state))
    .bind(schedule.local_date)
    .bind(schedule.scheduled_at)
    .bind(schedule.timezone)
    .bind(idempotency_key)
    .execute(&mut **transaction)
    .await
    .map_err(DaoError::Database)?;
    Ok(result.rows_affected() == 1)
}

async fn refresh_postgres_journey(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    action_id: &str,
) -> Result<(), DaoError> {
    // A habit's route-level progress is user-defined frequency progress, not
    // the ratio of an unbounded sequence of materialized occurrences.
    sqlx::query(
        "WITH context AS (SELECT journey_id FROM actions WHERE id = $1 AND user_id = $2), aggregates AS (SELECT c.journey_id, COALESCE(((COUNT(*) FILTER (WHERE a.state = 'completed') * 100) / NULLIF(COUNT(*), 0))::int, 0) AS value FROM context c JOIN actions a ON a.journey_id = c.journey_id AND a.user_id = $2 GROUP BY c.journey_id), next_action AS (SELECT a.payload ->> 'title' AS title FROM actions a JOIN context c ON c.journey_id = a.journey_id WHERE a.user_id = $2 AND a.state = 'pending' ORDER BY a.scheduled_at NULLS LAST, a.id LIMIT 1) UPDATE journeys AS j SET progress = CASE WHEN j.payload ->> 'journey_type' = 'habit' THEN j.progress ELSE aggregates.value END, payload = jsonb_set(jsonb_set(j.payload, '{progress}', to_jsonb(CASE WHEN j.payload ->> 'journey_type' = 'habit' THEN j.progress ELSE aggregates.value END), true), '{next_action}', to_jsonb(COALESCE(next_action.title, '路线已完成'::text)), true), updated_at = now() FROM aggregates LEFT JOIN next_action ON true WHERE j.id = aggregates.journey_id AND j.user_id = $2",
    )
    .bind(action_id)
    .bind(user_id)
    .execute(&mut **transaction)
    .await
    .map_err(DaoError::Database)?;
    Ok(())
}

async fn ensure_postgres_timezone(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    timezone: Option<&str>,
) -> Result<(), DaoError> {
    let Some(timezone) = timezone else {
        return Ok(());
    };
    let known = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1)",
    )
    .bind(timezone)
    .fetch_one(&mut **transaction)
    .await
    .map_err(DaoError::Database)?;
    if known {
        Ok(())
    } else {
        Err(DaoError::Schedule(format!(
            "unknown IANA timezone {timezone}"
        )))
    }
}

fn postgres_action_schedule(
    action: &pb::Action,
    existing_local_date: Option<Date>,
) -> Result<PostgresActionSchedule, DaoError> {
    match (&action.scheduled_for, &action.scheduled_timezone) {
        (None, None) => Ok(PostgresActionSchedule {
            local_date: existing_local_date.unwrap_or_else(|| OffsetDateTime::now_utc().date()),
            scheduled_at: None,
            timezone: None,
        }),
        (Some(timestamp), Some(timezone)) => {
            let scheduled_at = OffsetDateTime::parse(timestamp, &Rfc3339)
                .map_err(|error| DaoError::Schedule(error.to_string()))?;
            Ok(PostgresActionSchedule {
                local_date: scheduled_at.date(),
                scheduled_at: Some(scheduled_at),
                timezone: Some(timezone.clone()),
            })
        }
        _ => Err(DaoError::Schedule(
            "timestamp and timezone must be stored together".to_string(),
        )),
    }
}

fn action_local_date(action: &pb::Action) -> Option<Date> {
    action
        .scheduled_for
        .as_deref()
        .and_then(|timestamp| OffsetDateTime::parse(timestamp, &Rfc3339).ok())
        .map(|timestamp| timestamp.date())
}

fn upsert_memory_route_intent(
    state: &mut State,
    user_id: &str,
    route_id: &str,
    desired_active: bool,
    private_journey_id: Option<String>,
) -> pb::RouteParticipationIntent {
    let key = (user_id.to_string(), route_id.to_string());
    if let Some(intent) = state.route_participation_intents.get_mut(&key) {
        if intent.desired_active != desired_active
            || intent.private_journey_id != private_journey_id
        {
            intent.desired_active = desired_active;
            intent.private_journey_id = private_journey_id;
            intent.version = intent.version.saturating_add(1);
        }
        return intent.clone();
    }
    let intent = pb::RouteParticipationIntent {
        route_id: route_id.to_string(),
        desired_active,
        private_journey_id,
        version: 1,
    };
    state
        .route_participation_intents
        .insert(key, intent.clone());
    intent
}

async fn upsert_postgres_route_intent(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    route_id: &str,
    desired_active: bool,
    private_journey_id: Option<&str>,
) -> Result<pb::RouteParticipationIntent, DaoError> {
    let (desired_active, private_journey_id, version) =
        sqlx::query_as::<_, (bool, Option<String>, i64)>(
            "INSERT INTO route_participation_intents (user_id, route_id, private_journey_id, desired_active) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id, route_id) DO UPDATE SET private_journey_id = EXCLUDED.private_journey_id, desired_active = EXCLUDED.desired_active, version = CASE WHEN route_participation_intents.desired_active IS DISTINCT FROM EXCLUDED.desired_active OR route_participation_intents.private_journey_id IS DISTINCT FROM EXCLUDED.private_journey_id THEN route_participation_intents.version + 1 ELSE route_participation_intents.version END, attempts = CASE WHEN route_participation_intents.desired_active IS DISTINCT FROM EXCLUDED.desired_active OR route_participation_intents.private_journey_id IS DISTINCT FROM EXCLUDED.private_journey_id THEN 0 ELSE route_participation_intents.attempts END, available_at = CASE WHEN route_participation_intents.desired_active IS DISTINCT FROM EXCLUDED.desired_active OR route_participation_intents.private_journey_id IS DISTINCT FROM EXCLUDED.private_journey_id THEN now() ELSE route_participation_intents.available_at END, lease_until = CASE WHEN route_participation_intents.desired_active IS DISTINCT FROM EXCLUDED.desired_active OR route_participation_intents.private_journey_id IS DISTINCT FROM EXCLUDED.private_journey_id THEN NULL ELSE route_participation_intents.lease_until END, last_error = CASE WHEN route_participation_intents.desired_active IS DISTINCT FROM EXCLUDED.desired_active OR route_participation_intents.private_journey_id IS DISTINCT FROM EXCLUDED.private_journey_id THEN NULL ELSE route_participation_intents.last_error END, updated_at = now() RETURNING desired_active, private_journey_id, version",
        )
        .bind(user_id)
        .bind(route_id)
        .bind(private_journey_id)
        .bind(desired_active)
        .fetch_one(&mut **transaction)
        .await
        .map_err(DaoError::Database)?;
    Ok(pb::RouteParticipationIntent {
        route_id: route_id.to_string(),
        desired_active,
        private_journey_id,
        version: u64::try_from(version).unwrap_or_default(),
    })
}

fn validate_knowledge_journey(
    state: &State,
    user_id: &str,
    journey_id: Option<&str>,
) -> Result<(), DaoError> {
    if let Some(journey_id) = journey_id
        && !state
            .journey_owners
            .get(journey_id)
            .is_some_and(|owner| owner == user_id)
    {
        return Err(DaoError::KnowledgeReferenceNotFound(journey_id.to_string()));
    }
    Ok(())
}

async fn ensure_postgres_journey(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    journey_id: &str,
) -> Result<(), DaoError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM journeys WHERE id=$1 AND user_id=$2)",
    )
    .bind(journey_id)
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(DaoError::Database)?;
    if !exists {
        return Err(DaoError::KnowledgeReferenceNotFound(journey_id.to_string()));
    }
    Ok(())
}

fn apply_knowledge_update(
    resource: &mut pb::KnowledgeResource,
    request: pb::UpdateKnowledgeRequest,
) {
    if let Some(title) = request.title {
        resource.title = title.trim().to_string();
    }
    if let Some(creator) = request.creator {
        resource.creator = creator.trim().to_string();
    }
    if let Some(summary) = request.summary {
        resource.summary = summary.trim().to_string();
    }
    if let Some(kind) = request.kind {
        resource.kind = kind;
    }
    if let Some(status) = request.status {
        resource.status = status;
    }
    if let Some(source_url) = request.source_url {
        resource.source_url = non_empty(source_url);
    }
    if let Some(body) = request.body {
        resource.body = non_empty(body);
    }
    if let Some(tags) = request.tags {
        resource.tags = tags.values;
    }
    if let Some(journey_id) = request.journey_id {
        resource.journey_id = non_empty(journey_id);
    }
    if let Some(progress) = request.progress {
        resource.progress = progress;
        if progress == 100 {
            resource.status = pb::KnowledgeResourceStatus::Completed as i32;
        }
    }
    if let Some(current_position) = request.current_position {
        resource.current_position = current_position;
    }
    if let Some(reading_seconds) = request.reading_seconds {
        resource.reading_seconds = reading_seconds;
    }
    if let Some(bookmarks) = request.bookmarks {
        resource.bookmarks = bookmarks.values;
    }
    if let Some(last_opened_at) = request.last_opened_at {
        resource.last_opened_at = non_empty(last_opened_at);
    }
    resource.updated_at = current_timestamp();
}

fn same_knowledge_content(left: &pb::KnowledgeResource, right: &pb::KnowledgeResource) -> bool {
    left.title == right.title
        && left.creator == right.creator
        && left.summary == right.summary
        && left.kind == right.kind
        && left.status == right.status
        && left.source_url == right.source_url
        && left.body == right.body
        && left.tags == right.tags
        && left.journey_id == right.journey_id
        && left.source_content_id == right.source_content_id
}

fn same_action_content(left: &pb::Action, right: &pb::Action) -> bool {
    left.journey_id == right.journey_id
        && left.stage_id == right.stage_id
        && left.title == right.title
        && left.detail == right.detail
        && left.estimated_minutes == right.estimated_minutes
        && left.scheduled_label == right.scheduled_label
        && left.scheduled_for == right.scheduled_for
        && left.scheduled_timezone == right.scheduled_timezone
        && left.recurrence == right.recurrence
}

// Persist the normalized request shape separately from mutable Journey rows.
// This keeps retries stable after a user updates the Journey or its first action.
fn journey_idempotency_payload(
    journey: &pb::Journey,
    first_action: &pb::Action,
) -> serde_json::Value {
    let first_action_stage_position = first_action.stage_id.as_deref().and_then(|stage_id| {
        journey
            .stages
            .iter()
            .find(|stage| stage.id == stage_id)
            .map(|stage| stage.position)
    });
    serde_json::json!({
        "journey": {
            "title": &journey.title,
            "intent": &journey.intent,
            "domain": journey.domain,
            "journey_type": journey.journey_type,
            "completion_criteria": &journey.completion_criteria,
            "stages": journey.stages.iter().map(|stage| serde_json::json!({
                "title": &stage.title,
                "detail": &stage.detail,
                "completion_criteria": &stage.completion_criteria,
                "position": stage.position,
            })).collect::<Vec<_>>(),
            "duration_label": &journey.duration_label,
        },
        "first_action": {
            "title": &first_action.title,
            "detail": &first_action.detail,
            "estimated_minutes": first_action.estimated_minutes,
            "scheduled_label": &first_action.scheduled_label,
            "scheduled_for": &first_action.scheduled_for,
            "scheduled_timezone": &first_action.scheduled_timezone,
            "recurrence": &first_action.recurrence,
            "stage_position": first_action_stage_position,
        },
    })
}

fn same_entry_content(left: &pb::GrowthEntry, right: &pb::GrowthEntry) -> bool {
    left.action_id == right.action_id
        && left.journey_id == right.journey_id
        && left.body == right.body
        && left.mood == right.mood
        && left.duration_minutes == right.duration_minutes
        && left.quantity == right.quantity
        && left.location == right.location
        && left.photo_media_id == right.photo_media_id
        // Publication progresses asynchronously after the request commits. The
        // idempotency boundary is the user's original public/private intent.
        && entry_requested_publication(left) == entry_requested_publication(right)
}

fn entry_requested_publication(entry: &pb::GrowthEntry) -> bool {
    entry.publication_status != pb::EntryPublicationStatus::Private as i32
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn current_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn deserialize_rows<T: serde::de::DeserializeOwned>(
    rows: Vec<serde_json::Value>,
) -> Result<Vec<T>, DaoError> {
    rows.into_iter()
        .map(|payload| serde_json::from_value(payload).map_err(DaoError::Serialization))
        .collect()
}

fn format_status(status: i32) -> &'static str {
    match pb::JourneyStatus::try_from(status).unwrap_or(pb::JourneyStatus::Active) {
        pb::JourneyStatus::Active => "active",
        pb::JourneyStatus::Paused => "paused",
        pb::JourneyStatus::Completed => "completed",
    }
}

fn format_action_state(state: i32) -> &'static str {
    match pb::ActionState::try_from(state).unwrap_or(pb::ActionState::Pending) {
        pb::ActionState::Pending => "pending",
        pb::ActionState::Completed => "completed",
        pb::ActionState::Skipped => "skipped",
    }
}

fn format_knowledge_kind(kind: i32) -> &'static str {
    match pb::KnowledgeResourceKind::try_from(kind).unwrap_or(pb::KnowledgeResourceKind::Book) {
        pb::KnowledgeResourceKind::Book => "book",
        pb::KnowledgeResourceKind::Article => "article",
        pb::KnowledgeResourceKind::Course => "course",
        pb::KnowledgeResourceKind::Video => "video",
        pb::KnowledgeResourceKind::Link => "link",
        pb::KnowledgeResourceKind::Note => "note",
    }
}

fn format_knowledge_status(status: i32) -> &'static str {
    match pb::KnowledgeResourceStatus::try_from(status)
        .unwrap_or(pb::KnowledgeResourceStatus::Inbox)
    {
        pb::KnowledgeResourceStatus::Inbox => "inbox",
        pb::KnowledgeResourceStatus::Active => "active",
        pb::KnowledgeResourceStatus::Completed => "completed",
        pb::KnowledgeResourceStatus::Archived => "archived",
    }
}

fn notification_data_to_json(data: &HashMap<String, String>) -> serde_json::Value {
    serde_json::to_value(data).unwrap_or_else(|_| serde_json::Value::Object(Default::default()))
}

fn notification_data_from_json(
    data: serde_json::Value,
) -> Result<HashMap<String, String>, DaoError> {
    serde_json::from_value(data).map_err(DaoError::Serialization)
}

fn progress_to_i32(progress: u32) -> Result<i32, DaoError> {
    i32::try_from(progress)
        .map_err(|_| DaoError::Schedule("journey progress exceeds database range".to_string()))
}

fn count_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_payload_excludes_private_entry_and_route_metadata() {
        let entry = pb::GrowthEntry {
            id: "entry-1".to_string(),
            action_id: None,
            journey_id: Some("journey-1".to_string()),
            body: "A public reflection".to_string(),
            mood: pb::EntryMood::Calm as i32,
            duration_minutes: Some(30),
            quantity: Some("3 km".to_string()),
            location: Some("private location".to_string()),
            photo_url: None,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            published: false,
            publication_status: pb::EntryPublicationStatus::Pending as i32,
            public_content_id: None,
            publication_error: None,
            photo_media_id: Some("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b".to_string()),
        };
        let journey = pb::Journey {
            id: "journey-1".to_string(),
            title: "A private route title".to_string(),
            intent: String::new(),
            domain: pb::GrowthDomain::Learning as i32,
            journey_type: pb::JourneyType::Project as i32,
            completion_criteria: String::new(),
            stages: Vec::new(),
            status: pb::JourneyStatus::Active as i32,
            progress: 0,
            duration_label: "4 weeks".to_string(),
            next_action: String::new(),
            participant_count: 0,
        };

        let payload = entry_publication_payload("user-1", &entry, Some(&journey));

        assert_eq!(payload["body"], "A public reflection");
        assert_eq!(payload["domain"], pb::GrowthDomain::Learning as i32);
        assert!(payload.get("location").is_none());
        assert!(payload.get("mood").is_none());
        assert!(payload.get("quantity").is_none());
        assert!(payload.get("route_title").is_none());
        assert!(payload.get("route_duration").is_none());
        assert_eq!(
            payload["media_asset_ids"],
            serde_json::json!(["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b"])
        );
    }

    #[test]
    fn entry_idempotency_keeps_the_original_publication_intent_after_delivery() {
        let requested = pb::GrowthEntry {
            id: "entry-1".to_string(),
            action_id: None,
            journey_id: None,
            body: "A public reflection".to_string(),
            mood: pb::EntryMood::Calm as i32,
            duration_minutes: None,
            quantity: None,
            location: None,
            photo_url: None,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            published: false,
            publication_status: pb::EntryPublicationStatus::Pending as i32,
            public_content_id: None,
            publication_error: None,
            photo_media_id: None,
        };
        let delivered = pb::GrowthEntry {
            id: "entry-2".to_string(),
            publication_status: pb::EntryPublicationStatus::Published as i32,
            public_content_id: Some("post-1".to_string()),
            ..requested.clone()
        };

        assert!(same_entry_content(&delivered, &requested));
    }

    #[test]
    fn journey_idempotency_payload_ignores_generated_ids_but_keeps_stage_assignment() {
        let journey = pb::Journey {
            id: "journey-1".to_string(),
            title: "阅读计划".to_string(),
            intent: "保留稳定阅读时间".to_string(),
            domain: pb::GrowthDomain::Learning as i32,
            journey_type: pb::JourneyType::Project as i32,
            completion_criteria: "完成三次阅读".to_string(),
            stages: vec![pb::JourneyStage {
                id: "stage-1".to_string(),
                title: "建立节奏".to_string(),
                detail: String::new(),
                completion_criteria: "完成第一次阅读".to_string(),
                position: 0,
            }],
            status: pb::JourneyStatus::Active as i32,
            progress: 0,
            duration_label: "一周".to_string(),
            next_action: "读十分钟".to_string(),
            participant_count: 1,
        };
        let action = pb::Action {
            id: "action-1".to_string(),
            journey_id: journey.id.clone(),
            stage_id: Some("stage-1".to_string()),
            title: "读十分钟".to_string(),
            detail: String::new(),
            estimated_minutes: 10,
            scheduled_label: "今晚".to_string(),
            scheduled_for: None,
            scheduled_timezone: None,
            recurrence: None,
            state: pb::ActionState::Pending as i32,
        };
        let mut regenerated_journey = journey.clone();
        regenerated_journey.id = "journey-2".to_string();
        regenerated_journey.stages[0].id = "stage-2".to_string();
        let regenerated_action = pb::Action {
            id: "action-2".to_string(),
            journey_id: regenerated_journey.id.clone(),
            stage_id: Some("stage-2".to_string()),
            ..action.clone()
        };

        assert_eq!(
            journey_idempotency_payload(&journey, &action),
            journey_idempotency_payload(&regenerated_journey, &regenerated_action)
        );
    }
}

#[path = "memory_growth_dao.rs"]
mod memory_growth_dao;
pub(crate) use memory_growth_dao::MemoryGrowthDao;
#[path = "postgres_growth_dao.rs"]
mod postgres_growth_dao;
pub(crate) use postgres_growth_dao::PostgresGrowthDao;
