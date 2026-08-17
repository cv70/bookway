use std::collections::HashMap;

use crate::api::pb;
use async_trait::async_trait;
use thiserror::Error;
use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
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
pub(crate) trait GrowthRepository: Send + Sync {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<pb::Journey>, RepositoryError>;
    async fn create_journey(
        &self,
        user_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Journey, RepositoryError>;
    async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        journey: pb::Journey,
        actions: Vec<pb::Action>,
    ) -> Result<pb::Journey, RepositoryError>;
    async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<pb::RouteParticipationIntent, RepositoryError>;
    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<pb::JourneyDetail, RepositoryError>;
    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, RepositoryError>;
    async fn create_action(
        &self,
        user_id: &str,
        action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Action, RepositoryError>;
    async fn today(
        &self,
        user_id: &str,
        local_date: Date,
    ) -> Result<Vec<pb::Action>, RepositoryError>;
    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<pb::Action, RepositoryError>;
    async fn source_route_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, RepositoryError>;
    async fn source_knowledge_content_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, RepositoryError>;
    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: pb::UpdateActionRequest,
    ) -> Result<pb::Action, RepositoryError>;
    async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::ReminderPreference, RepositoryError>;
    async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: pb::UpdateReminderPreferencesRequest,
    ) -> Result<pb::ReminderPreference, RepositoryError>;
    async fn register_push_device(
        &self,
        user_id: &str,
        request: pb::RegisterPushDeviceRequest,
    ) -> Result<pb::PushDevice, RepositoryError>;
    async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), RepositoryError>;
    async fn list_notifications(
        &self,
        user_id: &str,
        request: pb::NotificationQueryRequest,
    ) -> Result<pb::NotificationPage, RepositoryError>;
    async fn create_notification(
        &self,
        user_id: &str,
        request: pb::CreateNotificationRequest,
    ) -> Result<pb::UserNotification, RepositoryError>;
    async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<pb::UserNotification, RepositoryError>;
    async fn list_entries(&self, user_id: &str) -> Result<Vec<pb::GrowthEntry>, RepositoryError>;
    async fn create_entry(
        &self,
        user_id: &str,
        entry: pb::GrowthEntry,
        idempotency_key: Option<String>,
    ) -> Result<pb::GrowthEntry, RepositoryError>;
    async fn retry_entry_publication(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<pb::GrowthEntry, RepositoryError>;
    async fn review_snapshot(&self, user_id: &str) -> Result<ReviewSnapshot, RepositoryError>;
    async fn save_weekly_review(
        &self,
        user_id: &str,
        review: pb::ReviewRecord,
    ) -> Result<pb::ReviewRecord, RepositoryError>;
    async fn list_knowledge(
        &self,
        user_id: &str,
        query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, RepositoryError>;
    async fn create_knowledge(
        &self,
        user_id: &str,
        resource: pb::KnowledgeResource,
        idempotency_key: Option<String>,
    ) -> Result<pb::KnowledgeResource, RepositoryError>;
    async fn start_knowledge_journey(
        &self,
        user_id: &str,
        resource_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
    ) -> Result<pb::KnowledgeJourney, RepositoryError>;
    async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        request: pb::UpdateKnowledgeRequest,
    ) -> Result<pb::KnowledgeResource, RepositoryError>;
}

pub(crate) struct ReviewSnapshot {
    pub(crate) journeys: Vec<pb::Journey>,
    pub(crate) actions: Vec<pb::Action>,
    pub(crate) entries: Vec<pb::GrowthEntry>,
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

pub(crate) struct MemoryGrowthRepository {
    state: RwLock<State>,
}

fn memory_journey_detail(
    state: &State,
    user_id: &str,
    journey_id: &str,
) -> Result<pb::JourneyDetail, RepositoryError> {
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
        .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
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

impl MemoryGrowthRepository {
    pub(crate) fn seeded() -> Self {
        let journeys = vec![
            pb::Journey {
                id: "journey-reading".to_string(),
                title: "读懂现代城市".to_string(),
                intent: "用阅读建立观察一座城市的方法".to_string(),
                domain: pb::GrowthDomain::Learning as i32,
                journey_type: super::api::pb::JourneyType::Project as i32,
                completion_criteria: "完成六周阅读与观察记录".to_string(),
                stages: Vec::new(),
                status: pb::JourneyStatus::Active as i32,
                progress: 36,
                duration_label: "6 周".to_string(),
                next_action: "阅读《看不见的城市》第三章".to_string(),
                participant_count: 1284,
            },
            pb::Journey {
                id: "journey-running".to_string(),
                title: "重新跑起来".to_string(),
                intent: "以不受伤的方式恢复规律运动".to_string(),
                domain: pb::GrowthDomain::Movement as i32,
                journey_type: super::api::pb::JourneyType::Habit as i32,
                completion_criteria: "在四周内建立稳定、可恢复的跑步节奏".to_string(),
                stages: Vec::new(),
                status: pb::JourneyStatus::Active as i32,
                progress: 58,
                duration_label: "4 周".to_string(),
                next_action: "轻松跑 25 分钟".to_string(),
                participant_count: 3276,
            },
        ];

        let actions = [
            pb::Action {
                id: "action-read-city".to_string(),
                journey_id: "journey-reading".to_string(),
                stage_id: None,
                title: "阅读第三章".to_string(),
                detail: "标记一个关于城市空间的观点".to_string(),
                estimated_minutes: 30,
                scheduled_label: "上午".to_string(),
                scheduled_for: None,
                scheduled_timezone: None,
                recurrence: None,
                state: pb::ActionState::Pending as i32,
            },
            pb::Action {
                id: "action-easy-run".to_string(),
                journey_id: "journey-running".to_string(),
                stage_id: None,
                title: "轻松跑 25 分钟".to_string(),
                detail: "保持可以自然说话的配速".to_string(),
                estimated_minutes: 25,
                scheduled_label: "傍晚".to_string(),
                scheduled_for: None,
                scheduled_timezone: None,
                recurrence: None,
                state: pb::ActionState::Pending as i32,
            },
            pb::Action {
                id: "action-stretch".to_string(),
                journey_id: "journey-running".to_string(),
                stage_id: None,
                title: "跑后拉伸".to_string(),
                detail: "完成小腿与髋部拉伸".to_string(),
                estimated_minutes: 8,
                scheduled_label: "傍晚".to_string(),
                scheduled_for: None,
                scheduled_timezone: None,
                recurrence: None,
                state: pb::ActionState::Completed as i32,
            },
        ]
        .into_iter()
        .map(|action| (action.id.clone(), action))
        .collect();
        let journey_owners = [
            ("journey-reading".to_string(), "demo-user".to_string()),
            ("journey-running".to_string(), "demo-user".to_string()),
        ]
        .into_iter()
        .collect();
        let action_owners = [
            ("action-read-city".to_string(), "demo-user".to_string()),
            ("action-easy-run".to_string(), "demo-user".to_string()),
            ("action-stretch".to_string(), "demo-user".to_string()),
        ]
        .into_iter()
        .collect();
        let notifications = vec![pb::UserNotification {
            id: "018f01e8-0000-7000-8000-000000000001".to_string(),
            kind: pb::NotificationKind::ActionReminder as i32,
            source_id: "memory-reminder-action-read-city".to_string(),
            title: "行动提醒".to_string(),
            body: "阅读第三章已经安排好了，准备好时从一个段落开始。".to_string(),
            data: [
                ("action_id".to_string(), "action-read-city".to_string()),
                ("journey_id".to_string(), "journey-reading".to_string()),
            ]
            .into_iter()
            .collect(),
            read_at: None,
            created_at: "2025-01-01T09:00:00Z".to_string(),
        }];
        let notification_owners = [(
            "018f01e8-0000-7000-8000-000000000001".to_string(),
            "demo-user".to_string(),
        )]
        .into_iter()
        .collect();

        Self {
            state: RwLock::new(State {
                journeys,
                actions,
                journey_owners,
                journey_idempotency: HashMap::new(),
                route_journeys: HashMap::new(),
                route_participation_intents: HashMap::new(),
                action_owners,
                action_idempotency: HashMap::new(),
                entries: Vec::new(),
                entry_owners: HashMap::new(),
                entry_idempotency: HashMap::new(),
                knowledge_resources: HashMap::new(),
                knowledge_owners: HashMap::new(),
                knowledge_idempotency: HashMap::new(),
                knowledge_sources: HashMap::new(),
                reminder_preferences: HashMap::new(),
                push_devices: HashMap::new(),
                notifications,
                notification_owners,
                weekly_reviews: HashMap::new(),
            }),
        }
    }
}

#[async_trait]
impl GrowthRepository for MemoryGrowthRepository {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<pb::Journey>, RepositoryError> {
        let state = self.state.read().await;
        Ok(state
            .journeys
            .iter()
            .filter(|journey| {
                state
                    .journey_owners
                    .get(&journey.id)
                    .is_some_and(|owner| owner == user_id)
            })
            .cloned()
            .collect())
    }

    async fn create_journey(
        &self,
        user_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Journey, RepositoryError> {
        let mut state = self.state.write().await;
        let request_payload = journey_idempotency_payload(&journey, &first_action);
        if let Some(key) = idempotency_key.as_deref()
            && let Some((journey_id, stored_payload)) = state
                .journey_idempotency
                .get(&(user_id.to_string(), key.to_string()))
        {
            let existing = state
                .journeys
                .iter()
                .find(|item| item.id == *journey_id)
                .cloned()
                .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.clone()))?;
            return if *stored_payload == request_payload {
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        state
            .journey_owners
            .insert(journey.id.clone(), user_id.to_string());
        state
            .action_owners
            .insert(first_action.id.clone(), user_id.to_string());
        state.actions.insert(first_action.id.clone(), first_action);
        if let Some(key) = idempotency_key {
            state.journey_idempotency.insert(
                (user_id.to_string(), key),
                (journey.id.clone(), request_payload),
            );
        }
        state.journeys.push(journey.clone());
        Ok(journey)
    }

    async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        journey: pb::Journey,
        actions: Vec<pb::Action>,
    ) -> Result<pb::Journey, RepositoryError> {
        let mut state = self.state.write().await;
        let key = (user_id.to_string(), source_route_id.to_string());
        if let Some(journey_id) = state.route_journeys.get(&key).cloned() {
            let existing = state
                .journeys
                .iter()
                .find(|item| item.id == journey_id)
                .cloned()
                .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.clone()))?;
            upsert_memory_route_intent(
                &mut state,
                user_id,
                source_route_id,
                true,
                Some(existing.id.clone()),
            );
            return Ok(existing);
        }
        state.route_journeys.insert(key, journey.id.clone());
        state
            .journey_owners
            .insert(journey.id.clone(), user_id.to_string());
        for action in actions {
            state
                .action_owners
                .insert(action.id.clone(), user_id.to_string());
            state.actions.insert(action.id.clone(), action);
        }
        state.journeys.push(journey.clone());
        upsert_memory_route_intent(
            &mut state,
            user_id,
            source_route_id,
            true,
            Some(journey.id.clone()),
        );
        Ok(journey)
    }

    async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<pb::RouteParticipationIntent, RepositoryError> {
        let mut state = self.state.write().await;
        if active
            && let Some(journey_id) = private_journey_id.as_deref()
            && !state
                .journey_owners
                .get(journey_id)
                .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::JourneyNotFound(journey_id.to_string()));
        }
        Ok(upsert_memory_route_intent(
            &mut state,
            user_id,
            route_id,
            active,
            active.then_some(private_journey_id).flatten(),
        ))
    }

    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<pb::JourneyDetail, RepositoryError> {
        let state = self.state.read().await;
        memory_journey_detail(&state, user_id, journey_id)
    }

    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .journey_owners
            .get(journey_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::JourneyNotFound(journey_id.to_string()));
        }
        let journey = state
            .journeys
            .iter_mut()
            .find(|journey| journey.id == journey_id)
            .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
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
        Ok(journey.clone())
    }

    async fn create_action(
        &self,
        user_id: &str,
        action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Action, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .journey_owners
            .get(&action.journey_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::JourneyNotFound(action.journey_id));
        }
        if let Some(key) = idempotency_key.as_deref()
            && let Some(action_id) = state
                .action_idempotency
                .get(&(user_id.to_string(), key.to_string()))
        {
            let existing = state
                .actions
                .get(action_id)
                .cloned()
                .ok_or_else(|| RepositoryError::ActionNotFound(action_id.clone()))?;
            return if same_action_content(&existing, &action) {
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        state
            .action_owners
            .insert(action.id.clone(), user_id.to_string());
        if let Some(key) = idempotency_key {
            state
                .action_idempotency
                .insert((user_id.to_string(), key), action.id.clone());
        }
        state.actions.insert(action.id.clone(), action.clone());
        Ok(action)
    }

    async fn today(
        &self,
        user_id: &str,
        local_date: Date,
    ) -> Result<Vec<pb::Action>, RepositoryError> {
        let state = self.state.read().await;
        let mut actions: Vec<_> = state
            .actions
            .values()
            .filter(|action| {
                state
                    .action_owners
                    .get(&action.id)
                    .is_some_and(|owner| owner == user_id)
            })
            .filter(|action| action_local_date(action).is_none_or(|date| date == local_date))
            .cloned()
            .collect();
        actions.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(actions)
    }

    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<pb::Action, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .action_owners
            .get(action_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::ActionNotFound(action_id.to_string()));
        }
        let current = state
            .actions
            .get(action_id)
            .cloned()
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        let successor = (current.state == pb::ActionState::Pending as i32)
            .then(|| recurring_successor(&current))
            .transpose()?
            .flatten();
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        action.state = pb::ActionState::Completed as i32;
        let updated = action.clone();
        let journey_id = updated.journey_id.clone();
        if let Some(successor) = successor {
            state
                .action_owners
                .insert(successor.id.clone(), user_id.to_string());
            state.actions.insert(successor.id.clone(), successor);
        }
        refresh_memory_journey(&mut state, &journey_id);
        Ok(updated)
    }

    async fn source_route_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, RepositoryError> {
        let state = self.state.read().await;
        Ok(state
            .route_journeys
            .iter()
            .find_map(|((owner, route_id), private_journey_id)| {
                (owner == user_id && private_journey_id == journey_id).then(|| route_id.clone())
            }))
    }

    async fn source_knowledge_content_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, RepositoryError> {
        let state = self.state.read().await;
        let mut source_content_id = None;
        for resource in state.knowledge_resources.values().filter(|resource| {
            resource.journey_id.as_deref() == Some(journey_id)
                && state
                    .knowledge_owners
                    .get(&resource.id)
                    .is_some_and(|owner| owner == user_id)
        }) {
            let Some(candidate) = resource.source_content_id.as_ref() else {
                continue;
            };
            if source_content_id
                .as_ref()
                .is_some_and(|existing: &String| existing != candidate)
            {
                // Multiple distinct sources cannot be truthfully attributed to
                // one action, so leave this completion unattributed.
                return Ok(None);
            }
            source_content_id = Some(candidate.clone());
        }
        Ok(source_content_id)
    }

    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: pb::UpdateActionRequest,
    ) -> Result<pb::Action, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .action_owners
            .get(action_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::ActionNotFound(action_id.to_string()));
        }
        let current = state
            .actions
            .get(action_id)
            .cloned()
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        let spawn_successor = request.state == Some(pb::ActionState::Skipped as i32)
            && current.state == pb::ActionState::Pending as i32;
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
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
        let updated = action.clone();
        let successor = spawn_successor
            .then(|| recurring_successor(&updated))
            .transpose()?
            .flatten();
        let journey_id = updated.journey_id.clone();
        if let Some(successor) = successor {
            state
                .action_owners
                .insert(successor.id.clone(), user_id.to_string());
            state.actions.insert(successor.id.clone(), successor);
        }
        refresh_memory_journey(&mut state, &journey_id);
        Ok(updated)
    }

    async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::ReminderPreference, RepositoryError> {
        let state = self.state.read().await;
        Ok(state
            .reminder_preferences
            .get(user_id)
            .cloned()
            .unwrap_or_else(default_reminder_preferences))
    }

    async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: pb::UpdateReminderPreferencesRequest,
    ) -> Result<pb::ReminderPreference, RepositoryError> {
        let preferences = reminder_preferences_from_request(request);
        let mut state = self.state.write().await;
        state
            .reminder_preferences
            .insert(user_id.to_string(), preferences.clone());
        Ok(preferences)
    }

    async fn register_push_device(
        &self,
        user_id: &str,
        request: pb::RegisterPushDeviceRequest,
    ) -> Result<pb::PushDevice, RepositoryError> {
        let device = pb::PushDevice {
            device_id: request.device_id,
            provider: request.provider,
            active: true,
            updated_at: now_rfc3339(),
        };
        let mut state = self.state.write().await;
        state.push_devices.insert(
            device.device_id.clone(),
            (user_id.to_string(), device.clone()),
        );
        Ok(device)
    }

    async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.write().await;
        let Some((owner, device)) = state.push_devices.get_mut(device_id) else {
            return Ok(());
        };
        if owner == user_id {
            device.active = false;
            device.updated_at = now_rfc3339();
        }
        Ok(())
    }

    async fn list_notifications(
        &self,
        user_id: &str,
        request: pb::NotificationQueryRequest,
    ) -> Result<pb::NotificationPage, RepositoryError> {
        let limit = notification_limit(request.limit);
        let cursor = request
            .cursor
            .as_deref()
            .map(parse_notification_cursor)
            .transpose()?;
        let state = self.state.read().await;
        let unread_count = state
            .notifications
            .iter()
            .filter(|notification| {
                notification.read_at.is_none()
                    && state
                        .notification_owners
                        .get(&notification.id)
                        .is_some_and(|owner| owner == user_id)
            })
            .count();
        let mut items = state
            .notifications
            .iter()
            .filter(|notification| {
                state
                    .notification_owners
                    .get(&notification.id)
                    .is_some_and(|owner| owner == user_id)
            })
            .filter(|notification| {
                !request.unread_only.unwrap_or(false) || notification.read_at.is_none()
            })
            .filter(|notification| {
                cursor.as_ref().is_none_or(|(created_at, id)| {
                    notification.created_at.as_str() < created_at.as_str()
                        || (notification.created_at.as_str() == created_at.as_str()
                            && notification.id.as_str() < id.as_str())
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().map(notification_cursor))
            .flatten();
        Ok(pb::NotificationPage {
            items,
            next_cursor,
            unread_count: count_u32(unread_count),
        })
    }

    async fn create_notification(
        &self,
        user_id: &str,
        request: pb::CreateNotificationRequest,
    ) -> Result<pb::UserNotification, RepositoryError> {
        let mut state = self.state.write().await;
        if let Some(notification) = state.notifications.iter().find(|notification| {
            notification.kind == request.kind && notification.source_id == request.source_id
        }) {
            if state
                .notification_owners
                .get(&notification.id)
                .is_some_and(|owner| owner == user_id)
            {
                return Ok(notification.clone());
            }
            return Err(RepositoryError::NotificationSourceConflict(
                request.source_id,
            ));
        }
        let notification = pb::UserNotification {
            id: Uuid::now_v7().to_string(),
            kind: request.kind,
            source_id: request.source_id,
            title: request.title,
            body: request.body,
            data: request.data,
            read_at: None,
            created_at: now_rfc3339(),
        };
        state
            .notification_owners
            .insert(notification.id.clone(), user_id.to_string());
        state.notifications.push(notification.clone());
        Ok(notification)
    }

    async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<pb::UserNotification, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .notification_owners
            .get(notification_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::NotificationNotFound(
                notification_id.to_string(),
            ));
        }
        let notification = state
            .notifications
            .iter_mut()
            .find(|notification| notification.id == notification_id)
            .ok_or_else(|| RepositoryError::NotificationNotFound(notification_id.to_string()))?;
        if notification.read_at.is_none() {
            notification.read_at = Some(now_rfc3339());
        }
        Ok(notification.clone())
    }

    async fn list_entries(&self, user_id: &str) -> Result<Vec<pb::GrowthEntry>, RepositoryError> {
        let state = self.state.read().await;
        let mut entries = state
            .entries
            .iter()
            .filter(|entry| {
                state
                    .entry_owners
                    .get(&entry.id)
                    .is_some_and(|owner| owner == user_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(entries)
    }

    async fn create_entry(
        &self,
        user_id: &str,
        entry: pb::GrowthEntry,
        idempotency_key: Option<String>,
    ) -> Result<pb::GrowthEntry, RepositoryError> {
        let mut state = self.state.write().await;
        if let Some(journey_id) = &entry.journey_id
            && !state
                .journey_owners
                .get(journey_id)
                .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::EntryReferenceNotFound(journey_id.clone()));
        }
        if let Some(action_id) = &entry.action_id {
            if !state
                .action_owners
                .get(action_id)
                .is_some_and(|owner| owner == user_id)
            {
                return Err(RepositoryError::EntryReferenceNotFound(action_id.clone()));
            }
            if let Some(journey_id) = &entry.journey_id
                && state
                    .actions
                    .get(action_id)
                    .is_none_or(|action| &action.journey_id != journey_id)
            {
                return Err(RepositoryError::EntryReferenceNotFound(action_id.clone()));
            }
        }
        if let Some(key) = idempotency_key.as_deref()
            && let Some(entry_id) = state
                .entry_idempotency
                .get(&(user_id.to_string(), key.to_string()))
        {
            let existing = state
                .entries
                .iter()
                .find(|existing| existing.id == *entry_id)
                .cloned()
                .ok_or_else(|| RepositoryError::EntryNotFound(entry_id.clone()))?;
            return if same_entry_content(&existing, &entry) {
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        state
            .entry_owners
            .insert(entry.id.clone(), user_id.to_string());
        if let Some(key) = idempotency_key {
            state
                .entry_idempotency
                .insert((user_id.to_string(), key), entry.id.clone());
        }
        state.entries.push(entry.clone());
        Ok(entry)
    }

    async fn retry_entry_publication(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<pb::GrowthEntry, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .entry_owners
            .get(entry_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::EntryNotFound(entry_id.to_string()));
        }
        let entry = state
            .entries
            .iter_mut()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| RepositoryError::EntryNotFound(entry_id.to_string()))?;
        if entry.publication_status != pb::EntryPublicationStatus::Failed as i32 {
            return Err(RepositoryError::EntryPublicationNotRetryable);
        }
        entry.publication_status = pb::EntryPublicationStatus::Pending as i32;
        entry.publication_error = None;
        entry.published = false;
        Ok(entry.clone())
    }

    async fn review_snapshot(&self, user_id: &str) -> Result<ReviewSnapshot, RepositoryError> {
        let state = self.state.read().await;
        Ok(ReviewSnapshot {
            journeys: state
                .journeys
                .iter()
                .filter(|journey| {
                    state
                        .journey_owners
                        .get(&journey.id)
                        .is_some_and(|owner| owner == user_id)
                })
                .cloned()
                .collect(),
            actions: state
                .actions
                .values()
                .filter(|action| {
                    state
                        .action_owners
                        .get(&action.id)
                        .is_some_and(|owner| owner == user_id)
                })
                .cloned()
                .collect(),
            entries: state
                .entries
                .iter()
                .filter(|entry| {
                    state
                        .entry_owners
                        .get(&entry.id)
                        .is_some_and(|owner| owner == user_id)
                })
                .cloned()
                .collect(),
        })
    }

    async fn save_weekly_review(
        &self,
        user_id: &str,
        review: pb::ReviewRecord,
    ) -> Result<pb::ReviewRecord, RepositoryError> {
        let mut state = self.state.write().await;
        let key = (
            user_id.to_string(),
            review
                .summary
                .as_ref()
                .map(|summary| summary.period_start.clone())
                .unwrap_or_default(),
            review
                .summary
                .as_ref()
                .map(|summary| summary.period_end.clone())
                .unwrap_or_default(),
        );
        if let Some(existing) = state.weekly_reviews.get_mut(&key) {
            // Preserve the historical generated snapshot and original creation time.
            existing.reflection = review.reflection;
            existing.next_focus = review.next_focus;
            existing.updated_at = review.updated_at;
            return Ok(existing.clone());
        }
        state.weekly_reviews.insert(key, review.clone());
        Ok(review)
    }

    async fn list_knowledge(
        &self,
        user_id: &str,
        query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, RepositoryError> {
        let state = self.state.read().await;
        let q = query.q.as_deref().map(str::to_lowercase);
        let mut resources = state
            .knowledge_resources
            .values()
            .filter(|resource| {
                state
                    .knowledge_owners
                    .get(&resource.id)
                    .is_some_and(|owner| owner == user_id)
                    && query.kind.is_none_or(|kind| resource.kind == kind)
                    && query.status.is_none_or(|status| resource.status == status)
                    && query.tag.as_deref().is_none_or(|tag| {
                        resource
                            .tags
                            .iter()
                            .any(|candidate| candidate.eq_ignore_ascii_case(tag))
                    })
                    && q.as_deref().is_none_or(|q| {
                        resource.title.to_lowercase().contains(q)
                            || resource.creator.to_lowercase().contains(q)
                            || resource.summary.to_lowercase().contains(q)
                            || resource
                                .body
                                .as_deref()
                                .is_some_and(|body| body.to_lowercase().contains(q))
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        resources.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(resources)
    }

    async fn create_knowledge(
        &self,
        user_id: &str,
        resource: pb::KnowledgeResource,
        idempotency_key: Option<String>,
    ) -> Result<pb::KnowledgeResource, RepositoryError> {
        let mut state = self.state.write().await;
        validate_knowledge_journey(&state, user_id, resource.journey_id.as_deref())?;
        if let Some(source_content_id) = resource.source_content_id.as_deref()
            && let Some(resource_id) = state
                .knowledge_sources
                .get(&(user_id.to_string(), source_content_id.to_string()))
        {
            return state
                .knowledge_resources
                .get(resource_id)
                .cloned()
                .ok_or_else(|| RepositoryError::KnowledgeNotFound(resource_id.clone()));
        }
        if let Some(key) = idempotency_key.as_deref()
            && let Some(resource_id) = state
                .knowledge_idempotency
                .get(&(user_id.to_string(), key.to_string()))
        {
            let existing = state
                .knowledge_resources
                .get(resource_id)
                .cloned()
                .ok_or_else(|| RepositoryError::KnowledgeNotFound(resource_id.clone()))?;
            return if same_knowledge_content(&existing, &resource) {
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        state
            .knowledge_owners
            .insert(resource.id.clone(), user_id.to_string());
        if let Some(key) = idempotency_key {
            state
                .knowledge_idempotency
                .insert((user_id.to_string(), key), resource.id.clone());
        }
        if let Some(source_content_id) = resource.source_content_id.clone() {
            state.knowledge_sources.insert(
                (user_id.to_string(), source_content_id),
                resource.id.clone(),
            );
        }
        state
            .knowledge_resources
            .insert(resource.id.clone(), resource.clone());
        Ok(resource)
    }

    async fn start_knowledge_journey(
        &self,
        user_id: &str,
        resource_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
    ) -> Result<pb::KnowledgeJourney, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .knowledge_owners
            .get(resource_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::KnowledgeNotFound(resource_id.to_string()));
        }
        let mut resource = state
            .knowledge_resources
            .get(resource_id)
            .cloned()
            .ok_or_else(|| RepositoryError::KnowledgeNotFound(resource_id.to_string()))?;
        if let Some(journey_id) = resource.journey_id.as_deref() {
            let journey = memory_journey_detail(&state, user_id, journey_id)?;
            return Ok(pb::KnowledgeJourney {
                resource: Some(resource),
                journey: Some(journey),
            });
        }

        resource.journey_id = Some(journey.id.clone());
        resource.status = pb::KnowledgeResourceStatus::Active as i32;
        resource.updated_at = current_timestamp();
        state
            .journey_owners
            .insert(journey.id.clone(), user_id.to_string());
        state
            .action_owners
            .insert(first_action.id.clone(), user_id.to_string());
        state
            .actions
            .insert(first_action.id.clone(), first_action.clone());
        state.journeys.push(journey.clone());
        state
            .knowledge_resources
            .insert(resource_id.to_string(), resource.clone());
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
    ) -> Result<pb::KnowledgeResource, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .knowledge_owners
            .get(resource_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::KnowledgeNotFound(resource_id.to_string()));
        }
        validate_knowledge_journey(&state, user_id, request.journey_id.as_deref())?;
        let resource = state
            .knowledge_resources
            .get_mut(resource_id)
            .ok_or_else(|| RepositoryError::KnowledgeNotFound(resource_id.to_string()))?;
        apply_knowledge_update(resource, request);
        Ok(resource.clone())
    }
}

pub(crate) struct PostgresGrowthRepository {
    pool: sqlx::PgPool,
}

impl PostgresGrowthRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GrowthRepository for PostgresGrowthRepository {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<pb::Journey>, RepositoryError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id = $1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
            .collect()
    }

    async fn create_journey(
        &self,
        user_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Journey, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let request_payload = journey_idempotency_payload(&journey, &first_action);
        let inserted = sqlx::query(
            "INSERT INTO journeys (id, user_id, payload, status, progress, idempotency_key, idempotency_payload) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING",
        )
        .bind(&journey.id)
        .bind(user_id)
        .bind(serde_json::to_value(&journey).map_err(RepositoryError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .bind(idempotency_key.as_deref())
        .bind(&request_payload)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if inserted.rows_affected() == 0 {
            let Some(idempotency_key) = idempotency_key.as_deref() else {
                return Err(RepositoryError::IdempotencyConflict);
            };
            let (payload, stored_request_payload) = sqlx::query_as::<_, (serde_json::Value, Option<serde_json::Value>)>(
                "SELECT payload, idempotency_payload FROM journeys WHERE user_id=$1 AND idempotency_key=$2 FOR UPDATE",
            )
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or(RepositoryError::IdempotencyConflict)?;
            let existing: pb::Journey =
                serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
            return if stored_request_payload.as_ref() == Some(&request_payload) {
                transaction
                    .commit()
                    .await
                    .map_err(RepositoryError::Database)?;
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        insert_postgres_action(&mut transaction, user_id, &first_action, None).await?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(journey)
    }

    async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        journey: pb::Journey,
        actions: Vec<pb::Action>,
    ) -> Result<pb::Journey, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        // Keep route-derived Journey creation exactly-once under concurrent retries.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1 || chr(31) || $2, 0))")
            .bind(user_id)
            .bind(source_route_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        if let Some(payload) = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id = $1 AND source_route_id = $2",
        )
        .bind(user_id)
        .bind(source_route_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?
        {
            let existing: pb::Journey =
                serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
            upsert_postgres_route_intent(
                &mut transaction,
                user_id,
                source_route_id,
                true,
                Some(&existing.id),
            )
            .await?;
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return Ok(existing);
        }
        sqlx::query(
            "INSERT INTO journeys (id, user_id, source_route_id, payload, status, progress) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&journey.id)
        .bind(user_id)
        .bind(source_route_id)
        .bind(serde_json::to_value(&journey).map_err(RepositoryError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
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
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(journey)
    }

    async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<pb::RouteParticipationIntent, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if active && let Some(journey_id) = private_journey_id.as_deref() {
            let owned = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM journeys WHERE id = $1 AND user_id = $2)",
            )
            .bind(journey_id)
            .bind(user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if !owned {
                return Err(RepositoryError::JourneyNotFound(journey_id.to_string()));
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
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(intent)
    }

    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<pb::JourneyDetail, RepositoryError> {
        let journey = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE id = $1 AND user_id = $2",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
        let journey = serde_json::from_value(journey).map_err(RepositoryError::Serialization)?;
        let actions = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE journey_id = $1 AND user_id = $2 ORDER BY created_at",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .into_iter()
        .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
        .collect::<Result<Vec<pb::Action>, _>>()?;
        Ok(pb::JourneyDetail { journey, actions })
    }

    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, RepositoryError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
        let mut journey: pb::Journey =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
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
        .bind(serde_json::to_value(&journey).map_err(RepositoryError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .bind(journey_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(journey)
    }

    async fn create_action(
        &self,
        user_id: &str,
        action: pb::Action,
        idempotency_key: Option<String>,
    ) -> Result<pb::Action, RepositoryError> {
        let journey_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM journeys WHERE id = $1 AND user_id = $2)",
        )
        .bind(&action.journey_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        if !journey_exists {
            return Err(RepositoryError::JourneyNotFound(action.journey_id));
        }
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let inserted = insert_postgres_action(
            &mut transaction,
            user_id,
            &action,
            idempotency_key.as_deref(),
        )
        .await?;
        if !inserted {
            let Some(idempotency_key) = idempotency_key.as_deref() else {
                return Err(RepositoryError::IdempotencyConflict);
            };
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM actions WHERE user_id=$1 AND idempotency_key=$2 FOR UPDATE",
            )
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or(RepositoryError::IdempotencyConflict)?;
            let existing: pb::Action =
                serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
            return if same_action_content(&existing, &action) {
                transaction
                    .commit()
                    .await
                    .map_err(RepositoryError::Database)?;
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(action)
    }

    async fn today(
        &self,
        user_id: &str,
        local_date: Date,
    ) -> Result<Vec<pb::Action>, RepositoryError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE user_id = $1 AND scheduled_for = $2 ORDER BY scheduled_at NULLS LAST, id",
        )
        .bind(user_id)
        .bind(local_date)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
            .collect()
    }

    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<pb::Action, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let (payload, schedule_revision) = sqlx::query_as::<_, (serde_json::Value, i32)>(
            "SELECT payload, schedule_revision FROM actions WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(action_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        let mut action: pb::Action =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        let successor = (action.state == pb::ActionState::Pending as i32)
            .then(|| recurring_successor(&action))
            .transpose()?
            .flatten();
        action.state = pb::ActionState::Completed as i32;
        sqlx::query(
            "UPDATE actions SET state = 'completed', payload = $1, updated_at = now() WHERE id = $2 AND user_id = $3",
        )
        .bind(serde_json::to_value(&action).map_err(RepositoryError::Serialization)?)
        .bind(action_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        sqlx::query(
            "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE action_id = $1 AND schedule_revision = $2 AND status IN ('queued', 'processing')",
        )
        .bind(action_id)
        .bind(schedule_revision)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if let Some(successor) = successor {
            insert_postgres_action(&mut transaction, user_id, &successor, None).await?;
        }
        refresh_postgres_journey(&mut transaction, user_id, action_id).await?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(action)
    }

    async fn source_route_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, RepositoryError> {
        sqlx::query_scalar::<_, String>(
            "SELECT source_route_id FROM journeys WHERE id = $1 AND user_id = $2 AND source_route_id IS NOT NULL",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)
    }

    async fn source_knowledge_content_id_for_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<Option<String>, RepositoryError> {
        let sources = sqlx::query_scalar::<_, String>(
            "SELECT source_content_id FROM knowledge_resources WHERE user_id=$1 AND journey_id=$2 AND source_content_id IS NOT NULL GROUP BY source_content_id LIMIT 2",
        )
        .bind(user_id)
        .bind(journey_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok((sources.len() == 1).then(|| sources[0].clone()))
    }

    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: pb::UpdateActionRequest,
    ) -> Result<pb::Action, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let (payload, stored_local_date, schedule_revision) = sqlx::query_as::<_, (serde_json::Value, Date, i32)>(
            "SELECT payload, scheduled_for, schedule_revision FROM actions WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(action_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        let mut action: pb::Action =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
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
        .bind(serde_json::to_value(&action).map_err(RepositoryError::Serialization)?)
        .bind(format_action_state(action.state))
        .bind(schedule.local_date)
        .bind(schedule.scheduled_at)
        .bind(schedule.timezone)
        .bind(next_schedule_revision)
        .bind(action_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if schedule_changed || action.state != pb::ActionState::Pending as i32 {
            sqlx::query(
                "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE action_id = $1 AND schedule_revision <= $2 AND status IN ('queued', 'processing')",
            )
            .bind(action_id)
            .bind(next_schedule_revision)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        if let Some(successor) = successor {
            insert_postgres_action(&mut transaction, user_id, &successor, None).await?;
        }
        refresh_postgres_journey(&mut transaction, user_id, action_id).await?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(action)
    }

    async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::ReminderPreference, RepositoryError> {
        let row = sqlx::query_as::<_, (bool, i16, String, Option<time::Time>, Option<time::Time>, OffsetDateTime)>(
            "SELECT enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end, updated_at FROM reminder_preferences WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        row.map(reminder_preferences_from_row)
            .transpose()?
            .map_or_else(|| Ok(default_reminder_preferences()), Ok)
    }

    async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: pb::UpdateReminderPreferencesRequest,
    ) -> Result<pb::ReminderPreference, RepositoryError> {
        let preferences = reminder_preferences_from_request(request);
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        ensure_postgres_timezone(&mut transaction, Some(&preferences.timezone)).await?;
        let row = sqlx::query_as::<_, (bool, i16, String, Option<time::Time>, Option<time::Time>, OffsetDateTime)>(
            "INSERT INTO reminder_preferences (user_id, enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id) DO UPDATE SET enabled = EXCLUDED.enabled, lead_minutes = EXCLUDED.lead_minutes, timezone = EXCLUDED.timezone, quiet_hours_start = EXCLUDED.quiet_hours_start, quiet_hours_end = EXCLUDED.quiet_hours_end, version = reminder_preferences.version + 1, updated_at = now() RETURNING enabled, lead_minutes, timezone, quiet_hours_start, quiet_hours_end, updated_at",
        )
        .bind(user_id)
        .bind(preferences.enabled)
        .bind(i16::try_from(preferences.lead_minutes).map_err(|_| {
            RepositoryError::Schedule("reminder lead minutes exceed database range".to_string())
        })?)
        .bind(&preferences.timezone)
        .bind(parse_quiet_time(preferences.quiet_hours_start.as_deref())?)
        .bind(parse_quiet_time(preferences.quiet_hours_end.as_deref())?)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if !preferences.enabled {
            sqlx::query(
            "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE user_id = $1 AND status IN ('queued', 'processing')",
            )
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        reminder_preferences_from_row(row)
    }

    async fn register_push_device(
        &self,
        user_id: &str,
        request: pb::RegisterPushDeviceRequest,
    ) -> Result<pb::PushDevice, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let row = sqlx::query_as::<_, (String, String, bool, OffsetDateTime)>(
            "INSERT INTO push_devices (user_id, device_id, provider, endpoint, active, revoked_at) VALUES ($1, $2, $3, $4, true, NULL) ON CONFLICT (device_id) DO UPDATE SET user_id = EXCLUDED.user_id, provider = EXCLUDED.provider, endpoint = EXCLUDED.endpoint, active = true, revoked_at = NULL, updated_at = now() RETURNING device_id, provider, active, updated_at",
        )
        .bind(user_id)
        .bind(&request.device_id)
        .bind(format_push_provider(request.provider))
        .bind(&request.endpoint)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        sqlx::query(
            "UPDATE reminder_deliveries SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() WHERE device_id = $1 AND user_id <> $2 AND status IN ('queued', 'processing')",
        )
        .bind(&request.device_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        push_device_from_row(row)
    }

    async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "WITH revoked AS (UPDATE push_devices SET active = false, revoked_at = now(), updated_at = now() WHERE user_id = $1 AND device_id = $2 AND active RETURNING device_id) UPDATE reminder_deliveries d SET status = 'canceled', canceled_at = now(), locked_at = NULL, lease_id = NULL, updated_at = now() FROM revoked r WHERE d.user_id = $1 AND d.device_id = r.device_id AND d.status IN ('queued', 'processing')",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(())
    }

    async fn list_notifications(
        &self,
        user_id: &str,
        request: pb::NotificationQueryRequest,
    ) -> Result<pb::NotificationPage, RepositoryError> {
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
            .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
        let cursor_id = cursor
            .as_ref()
            .map(|(_, id)| Uuid::parse_str(id))
            .transpose()
            .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
        let rows = sqlx::query_as::<_, (Uuid, String, String, String, String, serde_json::Value, Option<OffsetDateTime>, OffsetDateTime)>(
            "SELECT id, kind, source_id, title, body, data, read_at, created_at FROM user_notifications WHERE user_id = $1 AND ($2 = false OR read_at IS NULL) AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4::uuid)) ORDER BY created_at DESC, id DESC LIMIT $5",
        )
        .bind(user_id)
        .bind(request.unread_only.unwrap_or(false))
        .bind(cursor_time)
        .bind(cursor_id)
        .bind(i64::try_from(limit + 1).map_err(|_| {
            RepositoryError::Schedule("notification page size is invalid".to_string())
        })?)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
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
        .map_err(RepositoryError::Database)?;
        Ok(pb::NotificationPage {
            next_cursor: has_more
                .then(|| items.last().map(notification_cursor))
                .flatten(),
            items,
            unread_count: u32::try_from(unread_count).map_err(|_| {
                RepositoryError::Schedule("stored unread notification count is invalid".to_string())
            })?,
        })
    }

    async fn create_notification(
        &self,
        user_id: &str,
        request: pb::CreateNotificationRequest,
    ) -> Result<pb::UserNotification, RepositoryError> {
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
        .map_err(RepositoryError::Database)?
        .ok_or(RepositoryError::NotificationSourceConflict(request.source_id))?;
        user_notification_from_row(row)
    }

    async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<pb::UserNotification, RepositoryError> {
        let notification_id = Uuid::parse_str(notification_id)
            .map_err(|_| RepositoryError::NotificationNotFound(notification_id.to_string()))?;
        let row = sqlx::query_as::<_, (Uuid, String, String, String, String, serde_json::Value, Option<OffsetDateTime>, OffsetDateTime)>(
            "UPDATE user_notifications SET read_at = COALESCE(read_at, now()) WHERE id = $1 AND user_id = $2 RETURNING id, kind, source_id, title, body, data, read_at, created_at",
        )
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotificationNotFound(notification_id.to_string()))?;
        user_notification_from_row(row)
    }

    async fn list_entries(&self, user_id: &str) -> Result<Vec<pb::GrowthEntry>, RepositoryError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM growth_entries WHERE user_id=$1 ORDER BY created_at DESC LIMIT 500",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
            .collect()
    }

    async fn create_entry(
        &self,
        user_id: &str,
        entry: pb::GrowthEntry,
        idempotency_key: Option<String>,
    ) -> Result<pb::GrowthEntry, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let mut journey = None;
        if let Some(journey_id) = &entry.journey_id {
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM journeys WHERE id=$1 AND user_id=$2",
            )
            .bind(journey_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or_else(|| RepositoryError::EntryReferenceNotFound(journey_id.clone()))?;
            journey =
                Some(serde_json::from_value(payload).map_err(RepositoryError::Serialization)?);
        }
        if let Some(action_id) = &entry.action_id {
            let action_journey = sqlx::query_scalar::<_, String>(
                "SELECT journey_id FROM actions WHERE id=$1 AND user_id=$2",
            )
            .bind(action_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or_else(|| RepositoryError::EntryReferenceNotFound(action_id.clone()))?;
            if entry
                .journey_id
                .as_ref()
                .is_some_and(|journey_id| journey_id != &action_journey)
            {
                return Err(RepositoryError::EntryReferenceNotFound(action_id.clone()));
            }
        }
        let inserted = sqlx::query(
            "INSERT INTO growth_entries (id,user_id,journey_id,action_id,payload,published,created_at,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7::timestamptz,$8) ON CONFLICT DO NOTHING",
        )
        .bind(&entry.id)
        .bind(user_id)
        .bind(&entry.journey_id)
        .bind(&entry.action_id)
        .bind(serde_json::to_value(&entry).map_err(RepositoryError::Serialization)?)
        .bind(entry.published)
        .bind(&entry.created_at)
        .bind(idempotency_key.as_deref())
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if inserted.rows_affected() == 0 {
            let Some(idempotency_key) = idempotency_key.as_deref() else {
                return Err(RepositoryError::IdempotencyConflict);
            };
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM growth_entries WHERE user_id=$1 AND idempotency_key=$2 FOR UPDATE",
            )
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or(RepositoryError::IdempotencyConflict)?;
            let existing: pb::GrowthEntry =
                serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
            return if same_entry_content(&existing, &entry) {
                transaction
                    .commit()
                    .await
                    .map_err(RepositoryError::Database)?;
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
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
            .map_err(RepositoryError::Database)?;
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(entry)
    }

    async fn retry_entry_publication(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<pb::GrowthEntry, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM growth_entries WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(entry_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::EntryNotFound(entry_id.to_string()))?;
        let mut entry: pb::GrowthEntry =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        if entry.publication_status != pb::EntryPublicationStatus::Failed as i32 {
            return Err(RepositoryError::EntryPublicationNotRetryable);
        }
        entry.publication_status = pb::EntryPublicationStatus::Pending as i32;
        entry.publication_error = None;
        entry.published = false;
        sqlx::query(
            "UPDATE growth_entries SET payload=$3,published=false WHERE id=$1 AND user_id=$2",
        )
        .bind(entry_id)
        .bind(user_id)
        .bind(serde_json::to_value(&entry).map_err(RepositoryError::Serialization)?)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let requeued = sqlx::query(
            "UPDATE entry_publication_jobs SET status='pending',attempts=0,available_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE entry_id=$1 AND user_id=$2 AND status='dead'",
        )
        .bind(entry_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if requeued.rows_affected() != 1 {
            return Err(RepositoryError::EntryPublicationNotRetryable);
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(entry)
    }

    async fn review_snapshot(&self, user_id: &str) -> Result<ReviewSnapshot, RepositoryError> {
        let journey_rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id=$1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let action_rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE user_id=$1 AND updated_at >= date_trunc('week',now())",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let entry_rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM growth_entries WHERE user_id=$1 AND created_at >= date_trunc('week',now()) ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
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
    ) -> Result<pb::ReviewRecord, RepositoryError> {
        let summary = review
            .summary
            .as_ref()
            .ok_or(RepositoryError::InvalidWeeklyReview)?;
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let inserted = sqlx::query(
            "INSERT INTO weekly_reviews (id,user_id,period_start,period_end,payload,created_at,updated_at) VALUES ($1,$2,$3::date,$4::date,$5,$6::timestamptz,$7::timestamptz) ON CONFLICT (user_id,period_start,period_end) DO NOTHING",
        )
        .bind(&review.id)
        .bind(user_id)
        .bind(&summary.period_start)
        .bind(&summary.period_end)
        .bind(serde_json::to_value(&review).map_err(RepositoryError::Serialization)?)
        .bind(&review.created_at)
        .bind(&review.updated_at)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if inserted.rows_affected() == 1 {
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
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
        .map_err(RepositoryError::Database)?
        .ok_or(RepositoryError::InvalidWeeklyReview)?;
        let mut existing: pb::ReviewRecord =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        // The first confirmed review is the immutable period snapshot. A
        // later PUT only edits the user's interpretation and next focus.
        existing.reflection = review.reflection;
        existing.next_focus = review.next_focus;
        existing.updated_at = review.updated_at;
        sqlx::query(
            "UPDATE weekly_reviews SET payload=$1, updated_at=$2::timestamptz WHERE id=$3 AND user_id=$4",
        )
        .bind(serde_json::to_value(&existing).map_err(RepositoryError::Serialization)?)
        .bind(&existing.updated_at)
        .bind(&existing.id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(existing)
    }

    async fn list_knowledge(
        &self,
        user_id: &str,
        query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, RepositoryError> {
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
        .map_err(RepositoryError::Database)?;
        deserialize_rows(rows)
    }

    async fn create_knowledge(
        &self,
        user_id: &str,
        resource: pb::KnowledgeResource,
        idempotency_key: Option<String>,
    ) -> Result<pb::KnowledgeResource, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
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
        .bind(serde_json::to_value(&resource).map_err(RepositoryError::Serialization)?)
        .bind(&resource.created_at)
        .bind(&resource.updated_at)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        if result.rows_affected() == 0 {
            let payload = if let Some(source_content_id) = resource.source_content_id.as_deref() {
                sqlx::query_scalar::<_, serde_json::Value>(
                    "SELECT payload FROM knowledge_resources WHERE user_id=$1 AND source_content_id=$2",
                )
                .bind(user_id)
                .bind(source_content_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(RepositoryError::Database)?
            } else if let Some(idempotency_key) = idempotency_key.as_deref() {
                sqlx::query_scalar::<_, serde_json::Value>(
                    "SELECT payload FROM knowledge_resources WHERE user_id=$1 AND idempotency_key=$2",
                )
                .bind(user_id)
                .bind(idempotency_key)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(RepositoryError::Database)?
            } else {
                None
            }
            .ok_or(RepositoryError::IdempotencyConflict)?;
            let existing: pb::KnowledgeResource =
                serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return if resource.source_content_id.is_some()
                || same_knowledge_content(&existing, &resource)
            {
                Ok(existing)
            } else {
                Err(RepositoryError::IdempotencyConflict)
            };
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(resource)
    }

    async fn start_knowledge_journey(
        &self,
        user_id: &str,
        resource_id: &str,
        journey: pb::Journey,
        first_action: pb::Action,
    ) -> Result<pb::KnowledgeJourney, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        // Lock the resource first. It is the per-user, per-resource idempotency
        // boundary, so concurrent retries cannot create two private Journeys.
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM knowledge_resources WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(resource_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::KnowledgeNotFound(resource_id.to_string()))?;
        let mut resource: pb::KnowledgeResource =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        if let Some(existing_journey_id) = resource.journey_id.as_deref() {
            let journey = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM journeys WHERE id=$1 AND user_id=$2",
            )
            .bind(existing_journey_id)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or_else(|| RepositoryError::JourneyNotFound(existing_journey_id.to_string()))?;
            let journey =
                serde_json::from_value(journey).map_err(RepositoryError::Serialization)?;
            let actions = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM actions WHERE journey_id=$1 AND user_id=$2 ORDER BY created_at",
            )
            .bind(existing_journey_id)
            .bind(user_id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?
            .into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
            .collect::<Result<Vec<pb::Action>, _>>()?;
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
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
        .bind(serde_json::to_value(&journey).map_err(RepositoryError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(progress_to_i32(journey.progress)?)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        insert_postgres_action(&mut transaction, user_id, &first_action, None).await?;

        resource.journey_id = Some(journey.id.clone());
        resource.status = pb::KnowledgeResourceStatus::Active as i32;
        resource.updated_at = current_timestamp();
        sqlx::query(
            "UPDATE knowledge_resources SET status=$1,journey_id=$2,payload=$3,updated_at=$4::timestamptz WHERE id=$5 AND user_id=$6",
        )
        .bind(format_knowledge_status(resource.status))
        .bind(&resource.journey_id)
        .bind(serde_json::to_value(&resource).map_err(RepositoryError::Serialization)?)
        .bind(&resource.updated_at)
        .bind(resource_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
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
    ) -> Result<pb::KnowledgeResource, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
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
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::KnowledgeNotFound(resource_id.to_string()))?;
        let mut resource: pb::KnowledgeResource =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        apply_knowledge_update(&mut resource, request);
        sqlx::query(
            "UPDATE knowledge_resources SET kind=$1,status=$2,title=$3,tags=$4,journey_id=$5,payload=$6,updated_at=$7::timestamptz WHERE id=$8 AND user_id=$9",
        )
        .bind(format_knowledge_kind(resource.kind))
        .bind(format_knowledge_status(resource.status))
        .bind(&resource.title)
        .bind(&resource.tags)
        .bind(&resource.journey_id)
        .bind(serde_json::to_value(&resource).map_err(RepositoryError::Serialization)?)
        .bind(&resource.updated_at)
        .bind(resource_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(resource)
    }
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
) -> Result<pb::ReminderPreference, RepositoryError> {
    Ok(pb::ReminderPreference {
        enabled,
        lead_minutes: u32::try_from(lead_minutes).map_err(|_| {
            RepositoryError::Schedule("stored reminder lead minutes is invalid".to_string())
        })?,
        timezone,
        quiet_hours_start: quiet_hours_start.map(format_quiet_time).transpose()?,
        quiet_hours_end: quiet_hours_end.map(format_quiet_time).transpose()?,
        updated_at: updated_at
            .format(&Rfc3339)
            .map_err(|error| RepositoryError::Schedule(error.to_string()))?,
    })
}

fn parse_quiet_time(value: Option<&str>) -> Result<Option<time::Time>, RepositoryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let format = time::format_description::parse_borrowed::<2>("[hour padding:zero]:[minute]")
        .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
    time::Time::parse(value, &format)
        .map(Some)
        .map_err(|error| RepositoryError::Schedule(error.to_string()))
}

fn format_quiet_time(value: time::Time) -> Result<String, RepositoryError> {
    let format = time::format_description::parse_borrowed::<2>("[hour padding:zero]:[minute]")
        .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
    value
        .format(&format)
        .map_err(|error| RepositoryError::Schedule(error.to_string()))
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
) -> Result<pb::PushDevice, RepositoryError> {
    let provider = match provider.as_str() {
        "expo" => pb::PushProvider::Expo,
        "fcm" => pb::PushProvider::Fcm,
        "apns" => pb::PushProvider::Apns,
        _ => {
            return Err(RepositoryError::Schedule(
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
            .map_err(|error| RepositoryError::Schedule(error.to_string()))?,
    })
}

fn notification_limit(value: Option<u32>) -> usize {
    usize::try_from(value.unwrap_or(30).clamp(1, 100)).unwrap_or(100)
}

fn parse_notification_cursor(value: &str) -> Result<(String, String), RepositoryError> {
    let (created_at, id) = value
        .split_once('|')
        .ok_or_else(|| RepositoryError::Schedule("notification cursor is invalid".to_string()))?;
    OffsetDateTime::parse(created_at, &Rfc3339)
        .map_err(|_| RepositoryError::Schedule("notification cursor is invalid".to_string()))?;
    Uuid::parse_str(id)
        .map_err(|_| RepositoryError::Schedule("notification cursor is invalid".to_string()))?;
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
) -> Result<pb::UserNotification, RepositoryError> {
    let kind = match kind.as_str() {
        "action_reminder" => pb::NotificationKind::ActionReminder,
        "community" => pb::NotificationKind::Community,
        "system" => pb::NotificationKind::System,
        _ => {
            return Err(RepositoryError::Schedule(
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
            .map_err(|error| RepositoryError::Schedule(error.to_string()))?,
        created_at: created_at
            .format(&Rfc3339)
            .map_err(|error| RepositoryError::Schedule(error.to_string()))?,
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
        if journey.journey_type != super::api::pb::JourneyType::Habit as i32 {
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

fn recurring_successor(action: &pb::Action) -> Result<Option<pb::Action>, RepositoryError> {
    let Some(recurrence) = action.recurrence.as_ref() else {
        return Ok(None);
    };
    let (Some(scheduled_for), Some(_scheduled_timezone)) = (
        action.scheduled_for.as_deref(),
        action.scheduled_timezone.as_deref(),
    ) else {
        return Err(RepositoryError::Schedule(
            "a recurring action must have a timestamp and timezone".to_string(),
        ));
    };
    let scheduled_at = OffsetDateTime::parse(scheduled_for, &Rfc3339)
        .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
    let anchor_date = recurrence
        .anchor_date
        .as_deref()
        .ok_or_else(|| RepositoryError::Schedule("recurrence anchor date is missing".to_string()))
        .and_then(repository_local_date)?;
    let end_date = recurrence
        .ends_on
        .as_deref()
        .map(repository_local_date)
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
            return Err(RepositoryError::Schedule(
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
        .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
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
) -> Result<Date, RepositoryError> {
    if weekdays.is_empty() {
        return Err(RepositoryError::Schedule(
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
    Err(RepositoryError::Schedule(
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

fn repository_local_date(value: &str) -> Result<Date, RepositoryError> {
    let format = time::format_description::parse_borrowed::<3>("[year]-[month]-[day]")
        .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
    Date::parse(value, &format).map_err(|error| RepositoryError::Schedule(error.to_string()))
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
) -> Result<bool, RepositoryError> {
    let schedule = postgres_action_schedule(action, None)?;
    ensure_postgres_timezone(transaction, schedule.timezone.as_deref()).await?;
    let result = sqlx::query(
        "INSERT INTO actions (id, journey_id, user_id, payload, state, scheduled_for, scheduled_at, scheduled_timezone, idempotency_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT DO NOTHING",
    )
    .bind(&action.id)
    .bind(&action.journey_id)
    .bind(user_id)
    .bind(serde_json::to_value(action).map_err(RepositoryError::Serialization)?)
    .bind(format_action_state(action.state))
    .bind(schedule.local_date)
    .bind(schedule.scheduled_at)
    .bind(schedule.timezone)
    .bind(idempotency_key)
    .execute(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    Ok(result.rows_affected() == 1)
}

async fn refresh_postgres_journey(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    action_id: &str,
) -> Result<(), RepositoryError> {
    // A habit's route-level progress is user-defined frequency progress, not
    // the ratio of an unbounded sequence of materialized occurrences.
    sqlx::query(
        "WITH context AS (SELECT journey_id FROM actions WHERE id = $1 AND user_id = $2), aggregates AS (SELECT c.journey_id, COALESCE(((COUNT(*) FILTER (WHERE a.state = 'completed') * 100) / NULLIF(COUNT(*), 0))::int, 0) AS value FROM context c JOIN actions a ON a.journey_id = c.journey_id AND a.user_id = $2 GROUP BY c.journey_id), next_action AS (SELECT a.payload ->> 'title' AS title FROM actions a JOIN context c ON c.journey_id = a.journey_id WHERE a.user_id = $2 AND a.state = 'pending' ORDER BY a.scheduled_at NULLS LAST, a.id LIMIT 1) UPDATE journeys AS j SET progress = CASE WHEN j.payload ->> 'journey_type' = 'habit' THEN j.progress ELSE aggregates.value END, payload = jsonb_set(jsonb_set(j.payload, '{progress}', to_jsonb(CASE WHEN j.payload ->> 'journey_type' = 'habit' THEN j.progress ELSE aggregates.value END), true), '{next_action}', to_jsonb(COALESCE(next_action.title, '路线已完成'::text)), true), updated_at = now() FROM aggregates LEFT JOIN next_action ON true WHERE j.id = aggregates.journey_id AND j.user_id = $2",
    )
    .bind(action_id)
    .bind(user_id)
    .execute(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    Ok(())
}

async fn ensure_postgres_timezone(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    timezone: Option<&str>,
) -> Result<(), RepositoryError> {
    let Some(timezone) = timezone else {
        return Ok(());
    };
    let known = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1)",
    )
    .bind(timezone)
    .fetch_one(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    if known {
        Ok(())
    } else {
        Err(RepositoryError::Schedule(format!(
            "unknown IANA timezone {timezone}"
        )))
    }
}

fn postgres_action_schedule(
    action: &pb::Action,
    legacy_local_date: Option<Date>,
) -> Result<PostgresActionSchedule, RepositoryError> {
    match (&action.scheduled_for, &action.scheduled_timezone) {
        (None, None) => Ok(PostgresActionSchedule {
            local_date: legacy_local_date.unwrap_or_else(|| OffsetDateTime::now_utc().date()),
            scheduled_at: None,
            timezone: None,
        }),
        (Some(timestamp), Some(timezone)) => {
            let scheduled_at = OffsetDateTime::parse(timestamp, &Rfc3339)
                .map_err(|error| RepositoryError::Schedule(error.to_string()))?;
            Ok(PostgresActionSchedule {
                local_date: scheduled_at.date(),
                scheduled_at: Some(scheduled_at),
                timezone: Some(timezone.clone()),
            })
        }
        _ => Err(RepositoryError::Schedule(
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
) -> Result<pb::RouteParticipationIntent, RepositoryError> {
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
        .map_err(RepositoryError::Database)?;
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
) -> Result<(), RepositoryError> {
    if let Some(journey_id) = journey_id
        && !state
            .journey_owners
            .get(journey_id)
            .is_some_and(|owner| owner == user_id)
    {
        return Err(RepositoryError::KnowledgeReferenceNotFound(
            journey_id.to_string(),
        ));
    }
    Ok(())
}

async fn ensure_postgres_journey(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    journey_id: &str,
) -> Result<(), RepositoryError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM journeys WHERE id=$1 AND user_id=$2)",
    )
    .bind(journey_id)
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(RepositoryError::Database)?;
    if !exists {
        return Err(RepositoryError::KnowledgeReferenceNotFound(
            journey_id.to_string(),
        ));
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
) -> Result<Vec<T>, RepositoryError> {
    rows.into_iter()
        .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
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
) -> Result<HashMap<String, String>, RepositoryError> {
    serde_json::from_value(data).map_err(RepositoryError::Serialization)
}

fn progress_to_i32(progress: u32) -> Result<i32, RepositoryError> {
    i32::try_from(progress).map_err(|_| {
        RepositoryError::Schedule("journey progress exceeds database range".to_string())
    })
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
