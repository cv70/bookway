use super::*;

pub(crate) struct MemoryGrowthDao {
    state: RwLock<State>,
}

impl MemoryGrowthDao {
    pub(crate) fn seeded() -> Self {
        let journeys = vec![
            pb::Journey {
                id: "journey-reading".to_string(),
                title: "读懂现代城市".to_string(),
                intent: "用阅读建立观察一座城市的方法".to_string(),
                domain: pb::GrowthDomain::Learning as i32,
                journey_type: crate::api::pb::JourneyType::Project as i32,
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
                journey_type: crate::api::pb::JourneyType::Habit as i32,
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
impl GrowthDao for MemoryGrowthDao {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<pb::Journey>, DaoError> {
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
    ) -> Result<pb::Journey, DaoError> {
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
                .ok_or_else(|| DaoError::JourneyNotFound(journey_id.clone()))?;
            return if *stored_payload == request_payload {
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
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
    ) -> Result<pb::Journey, DaoError> {
        let mut state = self.state.write().await;
        let key = (user_id.to_string(), source_route_id.to_string());
        if let Some(journey_id) = state.route_journeys.get(&key).cloned() {
            let existing = state
                .journeys
                .iter()
                .find(|item| item.id == journey_id)
                .cloned()
                .ok_or_else(|| DaoError::JourneyNotFound(journey_id.clone()))?;
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
    ) -> Result<pb::RouteParticipationIntent, DaoError> {
        let mut state = self.state.write().await;
        if active
            && let Some(journey_id) = private_journey_id.as_deref()
            && !state
                .journey_owners
                .get(journey_id)
                .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::JourneyNotFound(journey_id.to_string()));
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
    ) -> Result<pb::JourneyDetail, DaoError> {
        let state = self.state.read().await;
        memory_journey_detail(&state, user_id, journey_id)
    }

    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .journey_owners
            .get(journey_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::JourneyNotFound(journey_id.to_string()));
        }
        let journey = state
            .journeys
            .iter_mut()
            .find(|journey| journey.id == journey_id)
            .ok_or_else(|| DaoError::JourneyNotFound(journey_id.to_string()))?;
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
    ) -> Result<pb::Action, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .journey_owners
            .get(&action.journey_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::JourneyNotFound(action.journey_id));
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
                .ok_or_else(|| DaoError::ActionNotFound(action_id.clone()))?;
            return if same_action_content(&existing, &action) {
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
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

    async fn today(&self, user_id: &str, local_date: Date) -> Result<Vec<pb::Action>, DaoError> {
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
    ) -> Result<pb::Action, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .action_owners
            .get(action_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::ActionNotFound(action_id.to_string()));
        }
        let current = state
            .actions
            .get(action_id)
            .cloned()
            .ok_or_else(|| DaoError::ActionNotFound(action_id.to_string()))?;
        let successor = (current.state == pb::ActionState::Pending as i32)
            .then(|| recurring_successor(&current))
            .transpose()?
            .flatten();
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| DaoError::ActionNotFound(action_id.to_string()))?;
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
    ) -> Result<Option<String>, DaoError> {
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
    ) -> Result<Option<String>, DaoError> {
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
    ) -> Result<pb::Action, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .action_owners
            .get(action_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::ActionNotFound(action_id.to_string()));
        }
        let current = state
            .actions
            .get(action_id)
            .cloned()
            .ok_or_else(|| DaoError::ActionNotFound(action_id.to_string()))?;
        let spawn_successor = request.state == Some(pb::ActionState::Skipped as i32)
            && current.state == pb::ActionState::Pending as i32;
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| DaoError::ActionNotFound(action_id.to_string()))?;
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
    ) -> Result<pb::ReminderPreference, DaoError> {
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
    ) -> Result<pb::ReminderPreference, DaoError> {
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
    ) -> Result<pb::PushDevice, DaoError> {
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

    async fn revoke_push_device(&self, user_id: &str, device_id: &str) -> Result<(), DaoError> {
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
    ) -> Result<pb::NotificationPage, DaoError> {
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
    ) -> Result<pb::UserNotification, DaoError> {
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
            return Err(DaoError::NotificationSourceConflict(request.source_id));
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
    ) -> Result<pb::UserNotification, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .notification_owners
            .get(notification_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::NotificationNotFound(notification_id.to_string()));
        }
        let notification = state
            .notifications
            .iter_mut()
            .find(|notification| notification.id == notification_id)
            .ok_or_else(|| DaoError::NotificationNotFound(notification_id.to_string()))?;
        if notification.read_at.is_none() {
            notification.read_at = Some(now_rfc3339());
        }
        Ok(notification.clone())
    }

    async fn list_entries(&self, user_id: &str) -> Result<Vec<pb::GrowthEntry>, DaoError> {
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
    ) -> Result<pb::GrowthEntry, DaoError> {
        let mut state = self.state.write().await;
        if let Some(journey_id) = &entry.journey_id
            && !state
                .journey_owners
                .get(journey_id)
                .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::EntryReferenceNotFound(journey_id.clone()));
        }
        if let Some(action_id) = &entry.action_id {
            if !state
                .action_owners
                .get(action_id)
                .is_some_and(|owner| owner == user_id)
            {
                return Err(DaoError::EntryReferenceNotFound(action_id.clone()));
            }
            if let Some(journey_id) = &entry.journey_id
                && state
                    .actions
                    .get(action_id)
                    .is_none_or(|action| &action.journey_id != journey_id)
            {
                return Err(DaoError::EntryReferenceNotFound(action_id.clone()));
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
                .ok_or_else(|| DaoError::EntryNotFound(entry_id.clone()))?;
            return if same_entry_content(&existing, &entry) {
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
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
    ) -> Result<pb::GrowthEntry, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .entry_owners
            .get(entry_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::EntryNotFound(entry_id.to_string()));
        }
        let entry = state
            .entries
            .iter_mut()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| DaoError::EntryNotFound(entry_id.to_string()))?;
        if entry.publication_status != pb::EntryPublicationStatus::Failed as i32 {
            return Err(DaoError::EntryPublicationNotRetryable);
        }
        entry.publication_status = pb::EntryPublicationStatus::Pending as i32;
        entry.publication_error = None;
        entry.published = false;
        Ok(entry.clone())
    }

    async fn review_snapshot(&self, user_id: &str) -> Result<ReviewSnapshot, DaoError> {
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
    ) -> Result<pb::ReviewRecord, DaoError> {
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

    async fn apply_weekly_review_adjustment(
        &self,
        user_id: &str,
        review_id: &str,
        suggestion_index: u32,
    ) -> Result<AppliedReviewAdjustment, DaoError> {
        let mut state = self.state.write().await;
        let (review_key, stored_review) = state
            .weekly_reviews
            .iter()
            .find(|((owner, _, _), review)| owner == user_id && review.id == review_id)
            .map(|(key, review)| (key.clone(), review.clone()))
            .ok_or_else(|| DaoError::ReviewNotFound(review_id.to_string()))?;
        if let Some(decision) = stored_review
            .applied_adjustments
            .iter()
            .find(|decision| decision.suggestion_index == suggestion_index)
            .cloned()
        {
            return Ok(AppliedReviewAdjustment {
                review: stored_review,
                decision,
            });
        }
        let suggestion = stored_review
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
            if !state
                .action_owners
                .get(&patch.action_id)
                .is_some_and(|owner| owner == user_id)
            {
                return Err(DaoError::ActionNotFound(patch.action_id));
            }
            let action = state
                .actions
                .get_mut(&patch.action_id)
                .ok_or_else(|| DaoError::ActionNotFound(patch.action_id.clone()))?;
            if action.state != pb::ActionState::Pending as i32
                || action.estimated_minutes != expected_minutes
            {
                return Err(DaoError::ReviewAdjustmentStale);
            }
            action.estimated_minutes = proposed_minutes;
            pb::ReviewAdjustmentDecision {
                suggestion_index,
                applied_at: current_timestamp(),
                action: Some(action.clone()),
                journey: None,
            }
        } else if let Some(patch) = suggestion.journey_patch {
            let expected_status = patch
                .expected_status
                .ok_or(DaoError::ReviewAdjustmentStale)?;
            if !state
                .journey_owners
                .get(&patch.journey_id)
                .is_some_and(|owner| owner == user_id)
            {
                return Err(DaoError::JourneyNotFound(patch.journey_id));
            }
            let journey = state
                .journeys
                .iter_mut()
                .find(|journey| journey.id == patch.journey_id)
                .ok_or_else(|| DaoError::JourneyNotFound(patch.journey_id.clone()))?;
            if journey.status != expected_status {
                return Err(DaoError::ReviewAdjustmentStale);
            }
            journey.status = patch.status;
            pb::ReviewAdjustmentDecision {
                suggestion_index,
                applied_at: current_timestamp(),
                action: None,
                journey: Some(journey.clone()),
            }
        } else {
            return Err(DaoError::ReviewAdjustmentStale);
        };
        let review = state
            .weekly_reviews
            .get_mut(&review_key)
            .ok_or_else(|| DaoError::ReviewNotFound(review_id.to_string()))?;
        review.applied_adjustments.push(decision.clone());
        review.updated_at = decision.applied_at.clone();
        Ok(AppliedReviewAdjustment {
            review: review.clone(),
            decision,
        })
    }

    async fn list_knowledge(
        &self,
        user_id: &str,
        query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, DaoError> {
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
    ) -> Result<pb::KnowledgeResource, DaoError> {
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
                .ok_or_else(|| DaoError::KnowledgeNotFound(resource_id.clone()));
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
                .ok_or_else(|| DaoError::KnowledgeNotFound(resource_id.clone()))?;
            return if same_knowledge_content(&existing, &resource) {
                Ok(existing)
            } else {
                Err(DaoError::IdempotencyConflict)
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
    ) -> Result<pb::KnowledgeJourney, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .knowledge_owners
            .get(resource_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::KnowledgeNotFound(resource_id.to_string()));
        }
        let mut resource = state
            .knowledge_resources
            .get(resource_id)
            .cloned()
            .ok_or_else(|| DaoError::KnowledgeNotFound(resource_id.to_string()))?;
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
    ) -> Result<pb::KnowledgeResource, DaoError> {
        let mut state = self.state.write().await;
        if !state
            .knowledge_owners
            .get(resource_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(DaoError::KnowledgeNotFound(resource_id.to_string()));
        }
        validate_knowledge_journey(&state, user_id, request.journey_id.as_deref())?;
        let resource = state
            .knowledge_resources
            .get_mut(resource_id)
            .ok_or_else(|| DaoError::KnowledgeNotFound(resource_id.to_string()))?;
        apply_knowledge_update(resource, request);
        Ok(resource.clone())
    }
}
