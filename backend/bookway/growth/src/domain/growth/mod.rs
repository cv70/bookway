use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::api::{
    ActionDto, ActionRecurrenceDto, ActionRecurrenceFrequencyDto, ActionStateDto,
    CompanionBriefDto, CompanionModeDto, CreateActionRequest, CreateGrowthEntryRequest,
    CreateJourneyRequest, CreateJourneyStageRequest, CreateKnowledgeResourceRequest,
    CreateUserNotificationRequest, GrowthDomainDto, GrowthEntryDto, JourneyDetailDto, JourneyDto,
    JourneyStageDto, JourneyStatusDto, JourneyTypeDto, KnowledgeQueryRequest, KnowledgeResourceDto,
    NotificationPageDto, NotificationQueryRequest, PushDeviceDto, RegisterPushDeviceRequest,
    ReminderPreferencesDto, ReviewActionPatchDto, ReviewAdjustmentKindDto,
    ReviewAdjustmentSuggestionDto, ReviewDomainProgressDto, ReviewJourneyPatchDto,
    RouteParticipationIntentDto, TodayDto, UpdateActionRequest, UpdateJourneyRequest,
    UpdateKnowledgeResourceRequest, UpdateReminderPreferencesRequest, UserNotificationDto,
    WeekdayDto, WeeklyReviewDto,
};
use crate::domain::{Domain, GrowthError};

impl Domain {
    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<JourneyDto>, GrowthError> {
        Ok(self.repository.list_journeys(user_id).await?)
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, GrowthError> {
        let (journey, first_action) = build_journey(request)?;
        Ok(self
            .repository
            .create_journey(user_id, journey, first_action)
            .await?)
    }

    pub(crate) async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, GrowthError> {
        validate_identifier("来源路线 ID", source_route_id)?;
        let (journey, first_action) = build_journey(request)?;
        Ok(self
            .repository
            .create_route_journey(user_id, source_route_id, journey, first_action)
            .await?)
    }

    pub(crate) async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<RouteParticipationIntentDto, GrowthError> {
        validate_identifier("路线 ID", route_id)?;
        if let Some(journey_id) = private_journey_id.as_deref() {
            validate_identifier("私人路线 ID", journey_id)?;
        }
        Ok(self
            .repository
            .set_route_participation_intent(
                user_id,
                route_id,
                active,
                active.then_some(private_journey_id).flatten(),
            )
            .await?)
    }

    pub(crate) async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, GrowthError> {
        Ok(self.repository.get_journey(user_id, journey_id).await?)
    }

    pub(crate) async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, GrowthError> {
        validate_journey_update(&request)?;
        Ok(self
            .repository
            .update_journey(user_id, journey_id, request)
            .await?)
    }

    pub(crate) async fn create_action(
        &self,
        user_id: &str,
        mut request: CreateActionRequest,
    ) -> Result<ActionDto, GrowthError> {
        validate_action(
            &request.title,
            request.estimated_minutes,
            &request.scheduled_label,
        )?;
        let (scheduled_for, scheduled_timezone) =
            normalize_schedule(request.scheduled_for, request.scheduled_timezone)?;
        let journey = self
            .repository
            .get_journey(user_id, &request.journey_id)
            .await?;
        validate_action_stage(request.stage_id.as_deref(), &journey.journey.stages)?;
        request.recurrence = normalize_recurrence(
            request.recurrence,
            scheduled_for.as_deref(),
            scheduled_timezone.as_deref(),
        )?;
        let action = ActionDto {
            id: Uuid::now_v7().to_string(),
            journey_id: request.journey_id,
            stage_id: request.stage_id.map(|value| value.trim().to_string()),
            title: request.title.trim().to_string(),
            detail: request.detail.trim().to_string(),
            estimated_minutes: request.estimated_minutes,
            scheduled_label: request.scheduled_label.trim().to_string(),
            scheduled_for,
            scheduled_timezone,
            recurrence: request.recurrence,
            state: ActionStateDto::Pending,
        };
        Ok(self.repository.create_action(user_id, action).await?)
    }

    pub(crate) async fn today_for(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<TodayDto, GrowthError> {
        let schedule_context = schedule_context(local_date, timezone)?;
        let actions = self
            .repository
            .today(user_id, schedule_context.local_date)
            .await?;
        let completed = actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Completed)
            .count();
        let focus_minutes = actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Completed)
            .map(|action| u32::from(action.estimated_minutes))
            .sum();
        Ok(TodayDto {
            completed,
            total: actions.len(),
            focus_minutes,
            actions,
        })
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, GrowthError> {
        Ok(self.repository.complete_action(user_id, action_id).await?)
    }

    pub(crate) async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        mut request: UpdateActionRequest,
    ) -> Result<ActionDto, GrowthError> {
        if let Some(title) = &request.title
            && title.trim().is_empty()
        {
            return Err(GrowthError::Validation("行动名称不能为空".to_string()));
        }
        if let Some(minutes) = request.estimated_minutes
            && (minutes == 0 || minutes > 720)
        {
            return Err(GrowthError::Validation(
                "行动时长需要在 1 到 720 分钟之间".to_string(),
            ));
        }
        if request
            .scheduled_label
            .as_deref()
            .is_some_and(|label| label.trim().is_empty())
        {
            return Err(GrowthError::Validation("安排时间不能为空".to_string()));
        }
        if request.state == Some(ActionStateDto::Completed) {
            return Err(GrowthError::Validation(
                "请使用完成行动接口，以便保留重复计划的下一次安排".to_string(),
            ));
        }
        if request.scheduled_for.is_some() || request.scheduled_timezone.is_some() {
            let (scheduled_for, scheduled_timezone) =
                normalize_schedule(request.scheduled_for, request.scheduled_timezone)?;
            request.scheduled_for = scheduled_for;
            request.scheduled_timezone = scheduled_timezone;
        }
        Ok(self
            .repository
            .update_action(user_id, action_id, request)
            .await?)
    }

    pub(crate) async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<ReminderPreferencesDto, GrowthError> {
        Ok(self.repository.reminder_preferences(user_id).await?)
    }

    pub(crate) async fn update_reminder_preferences(
        &self,
        user_id: &str,
        mut request: UpdateReminderPreferencesRequest,
    ) -> Result<ReminderPreferencesDto, GrowthError> {
        validate_reminder_preferences(&request)?;
        request.timezone = request.timezone.trim().to_string();
        request.quiet_hours_start = request
            .quiet_hours_start
            .map(|value| value.trim().to_string());
        request.quiet_hours_end = request
            .quiet_hours_end
            .map(|value| value.trim().to_string());
        Ok(self
            .repository
            .update_reminder_preferences(user_id, request)
            .await?)
    }

    pub(crate) async fn register_push_device(
        &self,
        user_id: &str,
        mut request: RegisterPushDeviceRequest,
    ) -> Result<PushDeviceDto, GrowthError> {
        validate_identifier("设备 ID", &request.device_id)?;
        validate_text(&request.endpoint, 1, 4_096, "推送地址")?;
        request.device_id = request.device_id.trim().to_string();
        request.endpoint = request.endpoint.trim().to_string();
        Ok(self
            .repository
            .register_push_device(user_id, request)
            .await?)
    }

    pub(crate) async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), GrowthError> {
        validate_identifier("设备 ID", device_id)?;
        self.repository
            .revoke_push_device(user_id, device_id)
            .await?;
        Ok(())
    }

    pub(crate) async fn list_notifications(
        &self,
        user_id: &str,
        mut request: NotificationQueryRequest,
    ) -> Result<NotificationPageDto, GrowthError> {
        request.cursor = normalize_notification_cursor(request.cursor)?;
        Ok(self.repository.list_notifications(user_id, request).await?)
    }

    pub(crate) async fn create_notification(
        &self,
        user_id: &str,
        mut request: CreateUserNotificationRequest,
    ) -> Result<UserNotificationDto, GrowthError> {
        validate_identifier("通知接收者 ID", user_id)?;
        validate_notification(&request)?;
        request.source_id = request.source_id.trim().to_string();
        request.title = request.title.trim().to_string();
        request.body = request.body.trim().to_string();
        Ok(self
            .repository
            .create_notification(user_id, request)
            .await?)
    }

    pub(crate) async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<UserNotificationDto, GrowthError> {
        Uuid::parse_str(notification_id)
            .map_err(|_| GrowthError::Validation("通知 ID 格式不正确".to_string()))?;
        Ok(self
            .repository
            .mark_notification_read(user_id, notification_id)
            .await?)
    }

    pub(crate) async fn list_entries(
        &self,
        user_id: &str,
    ) -> Result<Vec<GrowthEntryDto>, GrowthError> {
        Ok(self.repository.list_entries(user_id).await?)
    }

    pub(crate) async fn create_entry(
        &self,
        user_id: &str,
        request: CreateGrowthEntryRequest,
    ) -> Result<GrowthEntryDto, GrowthError> {
        validate_entry(&request)?;
        let entry = GrowthEntryDto {
            id: Uuid::now_v7().to_string(),
            action_id: request.action_id,
            journey_id: request.journey_id,
            body: request.body.trim().to_string(),
            mood: request.mood,
            duration_minutes: request.duration_minutes,
            quantity: trimmed_option(request.quantity),
            location: trimmed_option(request.location),
            photo_url: trimmed_option(request.photo_url),
            created_at: now_rfc3339(),
            published: request.published,
        };
        Ok(self.repository.create_entry(user_id, entry).await?)
    }

    pub(crate) async fn weekly_review(
        &self,
        user_id: &str,
    ) -> Result<WeeklyReviewDto, GrowthError> {
        let snapshot = self.repository.review_snapshot(user_id).await?;
        let completed_actions = snapshot
            .actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Completed)
            .count();
        let skipped_actions = snapshot
            .actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Skipped)
            .count();
        let decided_actions = completed_actions + skipped_actions;
        let completion_rate = if decided_actions == 0 {
            0.0
        } else {
            completed_actions as f64 / decided_actions as f64
        };
        let recorded_minutes: u32 = snapshot
            .entries
            .iter()
            .filter_map(|entry| entry.duration_minutes)
            .map(u32::from)
            .sum();
        let focus_minutes = if recorded_minutes > 0 {
            recorded_minutes
        } else {
            snapshot
                .actions
                .iter()
                .filter(|action| action.state == ActionStateDto::Completed)
                .map(|action| u32::from(action.estimated_minutes))
                .sum()
        };
        let journey_domains = snapshot
            .journeys
            .iter()
            .map(|journey| (journey.id.as_str(), journey.domain))
            .collect::<std::collections::HashMap<_, _>>();
        let mut domains = std::collections::HashMap::<GrowthDomainDto, (usize, usize)>::new();
        for action in &snapshot.actions {
            let Some(domain) = journey_domains.get(action.journey_id.as_str()) else {
                continue;
            };
            let counts = domains.entry(*domain).or_default();
            counts.1 += 1;
            counts.0 += usize::from(action.state == ActionStateDto::Completed);
        }
        let mut domains = domains
            .into_iter()
            .map(
                |(domain, (completed_actions, total_actions))| ReviewDomainProgressDto {
                    domain,
                    completed_actions,
                    total_actions,
                },
            )
            .collect::<Vec<_>>();
        domains.sort_by(|left, right| {
            right
                .completed_actions
                .cmp(&left.completed_actions)
                .then_with(|| domain_order(left.domain).cmp(&domain_order(right.domain)))
        });
        let now = time::OffsetDateTime::now_utc();
        let week_start =
            now.date() - time::Duration::days(i64::from(now.weekday().number_days_from_monday()));
        let adjustment_suggestions = review_adjustment_suggestions(
            &snapshot,
            completion_rate,
            completed_actions,
            skipped_actions,
        );
        Ok(WeeklyReviewDto {
            period_start: week_start.to_string(),
            period_end: now.date().to_string(),
            completed_actions,
            skipped_actions,
            focus_minutes,
            entry_count: snapshot.entries.len(),
            active_journeys: snapshot
                .journeys
                .iter()
                .filter(|journey| journey.status == JourneyStatusDto::Active)
                .count(),
            completion_rate,
            domains,
            reflection_prompts: reflection_prompts(completion_rate, snapshot.entries.is_empty()),
            adjustment_suggestions,
        })
    }

    pub(crate) async fn companion_brief_for(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<CompanionBriefDto, GrowthError> {
        let schedule_context = schedule_context(local_date, timezone)?;
        let (actions, snapshot) = tokio::try_join!(
            self.repository.today(user_id, schedule_context.local_date),
            self.repository.review_snapshot(user_id),
        )?;
        let active_journey_ids = snapshot
            .journeys
            .iter()
            .filter(|journey| journey.status == JourneyStatusDto::Active)
            .map(|journey| journey.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let active_journeys = active_journey_ids.len();
        let active_actions = actions
            .iter()
            .filter(|action| active_journey_ids.contains(action.journey_id.as_str()))
            .collect::<Vec<_>>();
        let completed_actions = active_actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Completed)
            .count();
        let skipped_actions = active_actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Skipped)
            .count();
        let overdue_action = snapshot
            .actions
            .iter()
            .filter(|action| active_journey_ids.contains(action.journey_id.as_str()))
            .filter(|action| action.state == ActionStateDto::Pending)
            .filter(|action| action_is_overdue(action))
            .min_by(|left, right| {
                left.scheduled_for
                    .cmp(&right.scheduled_for)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .cloned();
        let pending_action = overdue_action.clone().or_else(|| {
            active_actions
                .iter()
                .copied()
                .filter(|action| action.state == ActionStateDto::Pending)
                .min_by(|left, right| {
                    left.estimated_minutes
                        .cmp(&right.estimated_minutes)
                        .then_with(|| left.id.cmp(&right.id))
                })
                .cloned()
        });

        let (mode, headline, message, reason, suggested_minutes, reflection_prompt) = match (
            &pending_action,
            overdue_action.is_some(),
        ) {
            (Some(action), true) => {
                let minutes = recovery_minutes(action.estimated_minutes);
                (
                        CompanionModeDto::StartSmall,
                        format!("让「{}」重新变得可开始", action.title),
                        "这一步的原定时间已经过去，不需要补上它。先给自己一个足够轻的版本，再决定要不要继续。"
                            .to_string(),
                        "它是进行中路线里最早尚未完成、且已有明确安排时间的行动。".to_string(),
                        Some(minutes),
                        "此刻怎样把这一步缩小到可以开始？".to_string(),
                    )
            }
            (Some(action), false) if completed_actions == 0 && skipped_actions > 0 => {
                let minutes = recovery_minutes(action.estimated_minutes);
                (
                    CompanionModeDto::StartSmall,
                    format!("把「{}」缩小到 {minutes} 分钟", action.title),
                    "不需要补上错过的计划。只做一个足够轻的小版本，再决定要不要继续。".to_string(),
                    "你今天有已跳过的行动，因此优先选择了最短的待办作为恢复入口。".to_string(),
                    Some(minutes),
                    "什么会让这一步比原计划更容易开始？".to_string(),
                )
            }
            (Some(action), false) if completed_actions == 0 => {
                let minutes = recovery_minutes(action.estimated_minutes);
                (
                    CompanionModeDto::StartSmall,
                    format!("先给「{}」{minutes} 分钟", action.title),
                    "今天不必做完所有事。先让最小的一步发生，节奏会重新回来。".to_string(),
                    "它是当前待办中用时最短的一步，适合作为低压力的开始。".to_string(),
                    Some(minutes),
                    "开始前，怎样让环境更支持这一步？".to_string(),
                )
            }
            (Some(action), false) => (
                CompanionModeDto::KeepGoing,
                format!("下一步是「{}」", action.title),
                "已经走出的每一步都会留下来。按原有节奏继续，或根据状态自行缩小它。".to_string(),
                "你已经完成了今天的一部分行动，因此保留当前路线的下一步。".to_string(),
                Some(action.estimated_minutes),
                "今天已经发生的什么，让接下来的行动更顺一点？".to_string(),
            ),
            (None, _) if completed_actions > 0 => (
                CompanionModeDto::Celebrate,
                "今天的行动已经告一段落".to_string(),
                "不需要再追赶。若愿意，留下一句感受，让这次完成成为以后能看见的证据。".to_string(),
                "今天没有待办行动，且已有完成记录。".to_string(),
                None,
                "今天哪一个瞬间，最值得被记住？".to_string(),
            ),
            (None, _) if active_journeys > 0 => (
                CompanionModeDto::PlanNext,
                "今天可以留一点空白".to_string(),
                "你的路线仍在这里。想继续时，为它安排一个足够小、足够具体的下一步即可。"
                    .to_string(),
                "当前没有待办行动，但仍有进行中的路线。".to_string(),
                None,
                "下一次行动，怎样安排才更符合现在的生活节奏？".to_string(),
            ),
            (None, _) => (
                CompanionModeDto::PlanNext,
                "从一个想靠近的方向开始".to_string(),
                "不必一次规划很远。选择一条想尝试的路线，再为今天留下第一个小行动。".to_string(),
                "你还没有进行中的路线或待办行动。".to_string(),
                None,
                "最近有什么变化，是你愿意花一点时间靠近的？".to_string(),
            ),
        };

        Ok(CompanionBriefDto {
            mode,
            headline,
            message,
            reason,
            suggested_action: pending_action,
            suggested_minutes,
            completed_actions,
            total_actions: active_actions.len(),
            active_journeys,
            reflection_prompt,
        })
    }

    pub(crate) async fn list_knowledge(
        &self,
        user_id: &str,
        mut query: KnowledgeQueryRequest,
    ) -> Result<Vec<KnowledgeResourceDto>, GrowthError> {
        query.q = normalize_query_filter(query.q, "检索词")?;
        query.tag = normalize_query_filter(query.tag, "标签")?;
        Ok(self.repository.list_knowledge(user_id, query).await?)
    }

    pub(crate) async fn create_knowledge(
        &self,
        user_id: &str,
        request: CreateKnowledgeResourceRequest,
        idempotency_key: Option<String>,
    ) -> Result<KnowledgeResourceDto, GrowthError> {
        validate_knowledge_create(&request)?;
        let idempotency_key = normalize_idempotency_key(idempotency_key)?;
        let now = now_rfc3339();
        let resource = KnowledgeResourceDto {
            id: Uuid::now_v7().to_string(),
            title: request.title.trim().to_string(),
            creator: request.creator.trim().to_string(),
            summary: request.summary.trim().to_string(),
            kind: request.kind,
            status: request.status,
            source_url: trimmed_option(request.source_url),
            body: trimmed_option(request.body),
            tags: normalize_tags(request.tags),
            journey_id: trimmed_option(request.journey_id),
            progress: 0,
            current_position: 0,
            reading_seconds: 0,
            bookmarks: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            last_opened_at: None,
        };
        Ok(self
            .repository
            .create_knowledge(user_id, resource, idempotency_key)
            .await?)
    }

    pub(crate) async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        mut request: UpdateKnowledgeResourceRequest,
    ) -> Result<KnowledgeResourceDto, GrowthError> {
        validate_knowledge_update(&request)?;
        request.title = request.title.map(|value| value.trim().to_string());
        request.creator = request.creator.map(|value| value.trim().to_string());
        request.summary = request.summary.map(|value| value.trim().to_string());
        request.source_url = request.source_url.map(|value| value.trim().to_string());
        request.body = request.body.map(|value| value.trim().to_string());
        request.tags = request.tags.map(normalize_tags);
        request.journey_id = request.journey_id.map(|value| value.trim().to_string());
        request.bookmarks = request.bookmarks.map(|bookmarks| {
            bookmarks
                .into_iter()
                .map(|bookmark| bookmark.trim().to_string())
                .collect()
        });
        request.last_opened_at = request.last_opened_at.map(|value| value.trim().to_string());
        Ok(self
            .repository
            .update_knowledge(user_id, resource_id, request)
            .await?)
    }
}

fn build_journey(request: CreateJourneyRequest) -> Result<(JourneyDto, ActionDto), GrowthError> {
    if request.title.trim().is_empty() {
        return Err(GrowthError::Validation("路线名称不能为空".to_string()));
    }
    validate_text(&request.completion_criteria, 0, 500, "路线完成标准")?;
    if request.first_action_title.trim().is_empty() {
        return Err(GrowthError::Validation("第一个行动不能为空".to_string()));
    }
    if request.estimated_minutes == 0 || request.estimated_minutes > 720 {
        return Err(GrowthError::Validation(
            "行动时长需要在 1 到 720 分钟之间".to_string(),
        ));
    }
    let first_action_scheduled_label = request
        .first_action_scheduled_label
        .unwrap_or_else(|| "今天".to_string());
    let (first_action_scheduled_for, first_action_scheduled_timezone) = normalize_schedule(
        request.first_action_scheduled_for,
        request.first_action_scheduled_timezone,
    )?;
    let stages = build_stages(request.stages)?;
    let first_action_stage_id = request
        .first_action_stage_index
        .map(usize::from)
        .map(|index| {
            stages
                .get(index)
                .map(|stage| stage.id.clone())
                .ok_or_else(|| GrowthError::Validation("首个行动所属阶段不存在".to_string()))
        })
        .transpose()?;
    let first_action_recurrence = normalize_recurrence(
        request.first_action_recurrence,
        first_action_scheduled_for.as_deref(),
        first_action_scheduled_timezone.as_deref(),
    )?;
    validate_action(
        &request.first_action_title,
        request.estimated_minutes,
        &first_action_scheduled_label,
    )?;
    let journey_id = Uuid::now_v7().to_string();
    let journey = JourneyDto {
        id: journey_id.clone(),
        title: request.title.trim().to_string(),
        intent: request.intent.trim().to_string(),
        domain: request.domain,
        journey_type: request.journey_type,
        completion_criteria: normalized_completion_criteria(
            request.completion_criteria,
            request.journey_type,
        ),
        stages,
        status: JourneyStatusDto::Active,
        progress: 0,
        duration_label: request.duration_label,
        next_action: request.first_action_title.trim().to_string(),
        participant_count: 1,
    };
    let first_action = ActionDto {
        id: Uuid::now_v7().to_string(),
        journey_id,
        stage_id: first_action_stage_id,
        title: request.first_action_title.trim().to_string(),
        detail: request.first_action_detail.trim().to_string(),
        estimated_minutes: request.estimated_minutes,
        scheduled_label: first_action_scheduled_label.trim().to_string(),
        scheduled_for: first_action_scheduled_for,
        scheduled_timezone: first_action_scheduled_timezone,
        recurrence: first_action_recurrence,
        state: ActionStateDto::Pending,
    };
    Ok((journey, first_action))
}

fn build_stages(
    stages: Vec<CreateJourneyStageRequest>,
) -> Result<Vec<JourneyStageDto>, GrowthError> {
    if stages.len() > 12 {
        return Err(GrowthError::Validation(
            "路线阶段不能超过 12 个".to_string(),
        ));
    }
    stages
        .into_iter()
        .enumerate()
        .map(|(position, stage)| {
            validate_text(&stage.title, 1, 120, "阶段名称")?;
            validate_text(&stage.detail, 0, 1_000, "阶段说明")?;
            validate_text(&stage.completion_criteria, 0, 500, "阶段完成标准")?;
            Ok(JourneyStageDto {
                id: Uuid::now_v7().to_string(),
                title: stage.title.trim().to_string(),
                detail: stage.detail.trim().to_string(),
                completion_criteria: stage.completion_criteria.trim().to_string(),
                position: u16::try_from(position)
                    .map_err(|_| GrowthError::Validation("路线阶段数量无效".to_string()))?,
            })
        })
        .collect()
}

fn normalized_completion_criteria(value: String, journey_type: JourneyTypeDto) -> String {
    let value = value.trim();
    if !value.is_empty() {
        return value.to_string();
    }
    match journey_type {
        JourneyTypeDto::Habit => "在自己的周期内达到期望频率".to_string(),
        JourneyTypeDto::Project => "完成路线中的必要阶段和行动".to_string(),
        JourneyTypeDto::Quantity => "达到为这条路线设定的累计目标".to_string(),
        JourneyTypeDto::Travel => "完成行前、在途和归来后的关键经历".to_string(),
        JourneyTypeDto::Challenge => "在限定周期内满足这条挑战的条件".to_string(),
    }
}

fn validate_action_stage(
    stage_id: Option<&str>,
    stages: &[JourneyStageDto],
) -> Result<(), GrowthError> {
    let Some(stage_id) = stage_id else {
        return Ok(());
    };
    validate_identifier("阶段 ID", stage_id)?;
    if stages.iter().any(|stage| stage.id == stage_id.trim()) {
        Ok(())
    } else {
        Err(GrowthError::Validation("行动所属阶段不存在".to_string()))
    }
}

fn normalize_recurrence(
    recurrence: Option<ActionRecurrenceDto>,
    scheduled_for: Option<&str>,
    scheduled_timezone: Option<&str>,
) -> Result<Option<ActionRecurrenceDto>, GrowthError> {
    let Some(mut recurrence) = recurrence else {
        return Ok(None);
    };
    let (Some(scheduled_for), Some(_scheduled_timezone)) = (scheduled_for, scheduled_timezone)
    else {
        return Err(GrowthError::Validation(
            "重复行动需要明确的安排时间和时区".to_string(),
        ));
    };
    if recurrence.interval == 0 || recurrence.interval > 365 {
        return Err(GrowthError::Validation(
            "重复间隔需要在 1 到 365 之间".to_string(),
        ));
    }
    match recurrence.frequency {
        ActionRecurrenceFrequencyDto::Daily if !recurrence.weekdays.is_empty() => {
            return Err(GrowthError::Validation(
                "按日重复不能设置星期几".to_string(),
            ));
        }
        ActionRecurrenceFrequencyDto::Weekly if recurrence.weekdays.is_empty() => {
            return Err(GrowthError::Validation(
                "按周重复至少需要选择一个星期".to_string(),
            ));
        }
        _ => {}
    }
    let mut weekdays = recurrence.weekdays.clone();
    weekdays.sort_by_key(weekday_order);
    weekdays.dedup();
    if weekdays.len() != recurrence.weekdays.len() {
        return Err(GrowthError::Validation("重复星期不能重复".to_string()));
    }
    recurrence.weekdays = weekdays;
    let scheduled_at = OffsetDateTime::parse(scheduled_for, &Rfc3339)
        .map_err(|_| GrowthError::Validation("安排时间格式不正确".to_string()))?;
    let scheduled_date = scheduled_at.date();
    let anchor_date = match recurrence.anchor_date.as_deref() {
        Some(value) => parse_local_date(value.trim())?,
        None => scheduled_date,
    };
    if anchor_date > scheduled_date {
        return Err(GrowthError::Validation(
            "重复计划起始日期不能晚于首个行动".to_string(),
        ));
    }
    if let Some(ends_on) = recurrence.ends_on.as_deref() {
        let ends_on = parse_local_date(ends_on.trim())?;
        if ends_on < scheduled_date {
            return Err(GrowthError::Validation(
                "重复计划结束日期不能早于首个行动".to_string(),
            ));
        }
        recurrence.ends_on = Some(ends_on.to_string());
    }
    recurrence.anchor_date = Some(anchor_date.to_string());
    Ok(Some(recurrence))
}

fn weekday_order(weekday: &WeekdayDto) -> u8 {
    match weekday {
        WeekdayDto::Monday => 0,
        WeekdayDto::Tuesday => 1,
        WeekdayDto::Wednesday => 2,
        WeekdayDto::Thursday => 3,
        WeekdayDto::Friday => 4,
        WeekdayDto::Saturday => 5,
        WeekdayDto::Sunday => 6,
    }
}

fn validate_identifier(label: &str, value: &str) -> Result<(), GrowthError> {
    if value.trim().is_empty() {
        return Err(GrowthError::Validation(format!("{label}不能为空")));
    }
    if value.chars().count() > 160 {
        return Err(GrowthError::Validation(format!(
            "{label}不能超过 160 个字符"
        )));
    }
    Ok(())
}

fn validate_notification(request: &CreateUserNotificationRequest) -> Result<(), GrowthError> {
    // Interaction keys include both actor and target identifiers, each of which may be 160 chars.
    validate_text(&request.source_id, 1, 512, "通知来源 ID")?;
    validate_text(&request.title, 1, 120, "通知标题")?;
    validate_text(&request.body, 0, 1_000, "通知内容")?;
    let data_size = serde_json::to_vec(&request.data)
        .map_err(|error| GrowthError::Validation(format!("通知数据无效: {error}")))?
        .len();
    if data_size > 16 * 1024 {
        return Err(GrowthError::Validation(
            "通知数据不能超过 16 KiB".to_string(),
        ));
    }
    Ok(())
}

fn validate_knowledge_create(request: &CreateKnowledgeResourceRequest) -> Result<(), GrowthError> {
    validate_text(&request.title, 1, 200, "资源标题")?;
    validate_text(&request.creator, 0, 120, "作者或来源")?;
    validate_text(&request.summary, 0, 1_000, "摘要")?;
    validate_optional_text(request.source_url.as_deref(), 2_048, "来源地址")?;
    validate_optional_text(request.body.as_deref(), 500_000, "资源正文")?;
    validate_tags(&request.tags)?;
    validate_optional_text(request.journey_id.as_deref(), 128, "路线标识")
}

fn validate_knowledge_update(request: &UpdateKnowledgeResourceRequest) -> Result<(), GrowthError> {
    if let Some(title) = request.title.as_deref() {
        validate_text(title, 1, 200, "资源标题")?;
    }
    if let Some(creator) = request.creator.as_deref() {
        validate_text(creator, 0, 120, "作者或来源")?;
    }
    if let Some(summary) = request.summary.as_deref() {
        validate_text(summary, 0, 1_000, "摘要")?;
    }
    validate_optional_text(request.source_url.as_deref(), 2_048, "来源地址")?;
    validate_optional_text(request.body.as_deref(), 500_000, "资源正文")?;
    validate_optional_text(request.journey_id.as_deref(), 128, "路线标识")?;
    if let Some(tags) = request.tags.as_deref() {
        validate_tags(tags)?;
    }
    if request.progress.is_some_and(|progress| progress > 100) {
        return Err(GrowthError::Validation(
            "阅读进度需要在 0 到 100 之间".to_string(),
        ));
    }
    if let Some(bookmarks) = request.bookmarks.as_deref() {
        if bookmarks.len() > 500 {
            return Err(GrowthError::Validation("书签不能超过 500 个".to_string()));
        }
        for bookmark in bookmarks {
            validate_text(bookmark, 1, 200, "书签")?;
        }
    }
    if let Some(value) = request.last_opened_at.as_deref() {
        time::OffsetDateTime::parse(value.trim(), &time::format_description::well_known::Rfc3339)
            .map_err(|_| GrowthError::Validation("最后打开时间必须是 RFC3339 格式".to_string()))?;
    }
    Ok(())
}

fn validate_text(value: &str, min: usize, max: usize, field: &str) -> Result<(), GrowthError> {
    let length = value.trim().chars().count();
    if length < min || length > max {
        return Err(GrowthError::Validation(format!(
            "{field}长度需要在 {min} 到 {max} 个字符之间"
        )));
    }
    Ok(())
}

fn validate_optional_text(value: Option<&str>, max: usize, field: &str) -> Result<(), GrowthError> {
    if let Some(value) = value
        && value.trim().chars().count() > max
    {
        return Err(GrowthError::Validation(format!(
            "{field}不能超过 {max} 个字符"
        )));
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> Result<(), GrowthError> {
    if tags.len() > 20 {
        return Err(GrowthError::Validation("标签不能超过 20 个".to_string()));
    }
    for tag in tags {
        validate_text(tag, 1, 40, "标签")?;
    }
    Ok(())
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim().to_string();
        if !normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&tag))
        {
            normalized.push(tag);
        }
    }
    normalized
}

fn normalize_query_filter(
    value: Option<String>,
    field: &str,
) -> Result<Option<String>, GrowthError> {
    let value = trimmed_option(value);
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > 100)
    {
        return Err(GrowthError::Validation(format!(
            "{field}不能超过 100 个字符"
        )));
    }
    Ok(value)
}

fn normalize_notification_cursor(value: Option<String>) -> Result<Option<String>, GrowthError> {
    let value = trimmed_option(value);
    if value
        .as_deref()
        .is_some_and(|cursor| cursor.chars().count() > 256)
    {
        return Err(GrowthError::Validation(
            "通知分页游标不能超过 256 个字符".to_string(),
        ));
    }
    Ok(value)
}

fn normalize_idempotency_key(value: Option<String>) -> Result<Option<String>, GrowthError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() || value.chars().count() > 128 {
        return Err(GrowthError::Validation(
            "幂等键长度需要在 1 到 128 个字符之间".to_string(),
        ));
    }
    Ok(Some(value))
}

fn validate_entry(request: &CreateGrowthEntryRequest) -> Result<(), GrowthError> {
    if request.body.chars().count() > 5_000 {
        return Err(GrowthError::Validation(
            "记录正文不能超过 5000 个字符".to_string(),
        ));
    }
    if request
        .duration_minutes
        .is_some_and(|minutes| minutes > 1_440)
    {
        return Err(GrowthError::Validation(
            "单次记录时长不能超过 1440 分钟".to_string(),
        ));
    }
    if request
        .quantity
        .as_deref()
        .is_some_and(|value| value.chars().count() > 80)
        || request
            .location
            .as_deref()
            .is_some_and(|value| value.chars().count() > 120)
    {
        return Err(GrowthError::Validation("数量或地点信息过长".to_string()));
    }
    if request
        .photo_url
        .as_deref()
        .is_some_and(|value| value.chars().count() > 2_048)
    {
        return Err(GrowthError::Validation("图片地址过长".to_string()));
    }
    if request.body.trim().is_empty()
        && request.duration_minutes.is_none()
        && request
            .quantity
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && request
            .photo_url
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(GrowthError::Validation(
            "文字、时长、数量和图片至少填写一项".to_string(),
        ));
    }
    Ok(())
}

fn trimmed_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn domain_order(domain: GrowthDomainDto) -> u8 {
    match domain {
        GrowthDomainDto::Learning => 0,
        GrowthDomainDto::Movement => 1,
        GrowthDomainDto::Wellness => 2,
        GrowthDomainDto::Travel => 3,
        GrowthDomainDto::Leisure => 4,
    }
}

fn reflection_prompts(completion_rate: f64, no_entries: bool) -> Vec<String> {
    let mut prompts = vec!["哪一步最值得保留到下周？".to_string()];
    if completion_rate < 0.5 {
        prompts.push("哪项计划可以缩小到更容易开始？".to_string());
    } else {
        prompts.push("是什么条件让这周的行动更容易发生？".to_string());
    }
    if no_entries {
        prompts.push("完成行动后，留下一句话作为成长证据。".to_string());
    } else {
        prompts.push("从本周记录里，你看见了什么重复出现的线索？".to_string());
    }
    prompts
}

fn review_adjustment_suggestions(
    snapshot: &crate::datasource::ReviewSnapshot,
    completion_rate: f64,
    completed_actions: usize,
    skipped_actions: usize,
) -> Vec<ReviewAdjustmentSuggestionDto> {
    let mut suggestions = Vec::new();
    let decided_actions = completed_actions + skipped_actions;
    if completion_rate < 0.5
        && (skipped_actions > 0 || decided_actions >= 3)
        && let Some(action) = snapshot
            .actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Pending)
            .max_by_key(|action| action.estimated_minutes)
    {
        let suggested_minutes = recovery_minutes(action.estimated_minutes);
        suggestions.push(ReviewAdjustmentSuggestionDto {
            kind: ReviewAdjustmentKindDto::ReduceActionDuration,
            title: format!("把「{}」先缩小到 {suggested_minutes} 分钟", action.title),
            rationale: "这周出现了中断。缩小下一步不会抹掉原计划，只是为恢复节奏留出更低的门槛。"
                .to_string(),
            action_patch: Some(ReviewActionPatchDto {
                action_id: action.id.clone(),
                estimated_minutes: Some(suggested_minutes),
                scheduled_label: None,
            }),
            journey_patch: None,
        });
    }

    for journey in snapshot
        .journeys
        .iter()
        .filter(|journey| journey.status == JourneyStatusDto::Active)
    {
        let actions = snapshot
            .actions
            .iter()
            .filter(|action| action.journey_id == journey.id)
            .collect::<Vec<_>>();
        if !actions.is_empty()
            && actions
                .iter()
                .all(|action| action.state == ActionStateDto::Skipped)
        {
            suggestions.push(ReviewAdjustmentSuggestionDto {
                kind: ReviewAdjustmentKindDto::PauseJourney,
                title: format!("先暂停「{}」", journey.title),
                rationale:
                    "这条路线的行动都被跳过了。暂停是保留计划与记录的选择，准备好后仍可继续。"
                        .to_string(),
                action_patch: None,
                journey_patch: Some(ReviewJourneyPatchDto {
                    journey_id: journey.id.clone(),
                    status: JourneyStatusDto::Paused,
                }),
            });
        }
    }
    suggestions
}

fn recovery_minutes(estimated_minutes: u16) -> u16 {
    (estimated_minutes / 3).clamp(5, 15)
}

struct ScheduleContext {
    local_date: Date,
}

fn schedule_context(
    local_date: Option<&str>,
    timezone: Option<&str>,
) -> Result<ScheduleContext, GrowthError> {
    let timezone = timezone.unwrap_or("UTC").trim();
    validate_timezone(timezone)?;
    let local_date = match local_date.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => parse_local_date(value)?,
        None => OffsetDateTime::now_utc().date(),
    };
    Ok(ScheduleContext { local_date })
}

fn normalize_schedule(
    scheduled_for: Option<String>,
    scheduled_timezone: Option<String>,
) -> Result<(Option<String>, Option<String>), GrowthError> {
    match (scheduled_for, scheduled_timezone) {
        (None, None) => Ok((None, None)),
        (Some(timestamp), Some(timezone)) => {
            let timezone = timezone.trim();
            validate_timezone(timezone)?;
            let timestamp = timestamp.trim();
            let timestamp = OffsetDateTime::parse(timestamp, &Rfc3339).map_err(|_| {
                GrowthError::Validation("安排时间必须是带 UTC 偏移量的 RFC 3339 时间戳".to_string())
            })?;
            let timestamp = timestamp
                .format(&Rfc3339)
                .map_err(|_| GrowthError::Validation("安排时间无法格式化".to_string()))?;
            Ok((Some(timestamp), Some(timezone.to_string())))
        }
        _ => Err(GrowthError::Validation(
            "安排时间和时区必须同时提供".to_string(),
        )),
    }
}

fn parse_local_date(value: &str) -> Result<Date, GrowthError> {
    let format = time::format_description::parse_borrowed::<3>("[year]-[month]-[day]")
        .map_err(|_| GrowthError::Validation("日期格式配置无效".to_string()))?;
    Date::parse(value, &format)
        .map_err(|_| GrowthError::Validation("日期必须是 YYYY-MM-DD".to_string()))
}

fn validate_timezone(timezone: &str) -> Result<(), GrowthError> {
    let valid = timezone == "UTC"
        || timezone == "GMT"
        || (timezone.contains('/')
            && timezone.len() <= 64
            && timezone.split('/').all(|segment| {
                !segment.is_empty()
                    && segment.chars().all(|character| {
                        character.is_ascii_alphanumeric()
                            || matches!(character, '_' | '-' | '+' | '.')
                    })
            }));
    if valid {
        Ok(())
    } else {
        Err(GrowthError::Validation(
            "时区必须是 UTC 或 IANA 时区名，例如 Asia/Shanghai".to_string(),
        ))
    }
}

fn validate_reminder_preferences(
    request: &UpdateReminderPreferencesRequest,
) -> Result<(), GrowthError> {
    if request.lead_minutes > 1_440 {
        return Err(GrowthError::Validation(
            "提醒提前时间需要在 0 到 1440 分钟之间".to_string(),
        ));
    }
    validate_timezone(request.timezone.trim())?;
    match (
        request.quiet_hours_start.as_deref(),
        request.quiet_hours_end.as_deref(),
    ) {
        (None, None) => Ok(()),
        (Some(start), Some(end)) => {
            let start = parse_quiet_time(start)?;
            let end = parse_quiet_time(end)?;
            if start == end {
                return Err(GrowthError::Validation(
                    "静默时段开始和结束时间不能相同".to_string(),
                ));
            }
            Ok(())
        }
        _ => Err(GrowthError::Validation(
            "静默时段开始和结束时间必须同时提供".to_string(),
        )),
    }
}

fn parse_quiet_time(value: &str) -> Result<time::Time, GrowthError> {
    let format = time::format_description::parse_borrowed::<2>("[hour padding:zero]:[minute]")
        .map_err(|_| GrowthError::Validation("静默时段格式配置无效".to_string()))?;
    time::Time::parse(value.trim(), &format).map_err(|_| {
        GrowthError::Validation("静默时段必须是 24 小时制 HH:MM，例如 22:30".to_string())
    })
}

fn action_is_overdue(action: &ActionDto) -> bool {
    action
        .scheduled_for
        .as_deref()
        .and_then(|timestamp| OffsetDateTime::parse(timestamp, &Rfc3339).ok())
        .is_some_and(|timestamp| timestamp < OffsetDateTime::now_utc())
}

fn validate_journey_update(request: &UpdateJourneyRequest) -> Result<(), GrowthError> {
    if request
        .title
        .as_deref()
        .is_some_and(|title| title.trim().is_empty())
    {
        return Err(GrowthError::Validation("路线名称不能为空".to_string()));
    }
    if request
        .duration_label
        .as_deref()
        .is_some_and(|duration| duration.trim().is_empty())
    {
        return Err(GrowthError::Validation("路线周期不能为空".to_string()));
    }
    Ok(())
}

fn validate_action(
    title: &str,
    estimated_minutes: u16,
    scheduled_label: &str,
) -> Result<(), GrowthError> {
    if title.trim().is_empty() {
        return Err(GrowthError::Validation("行动名称不能为空".to_string()));
    }
    if estimated_minutes == 0 || estimated_minutes > 720 {
        return Err(GrowthError::Validation(
            "行动时长需要在 1 到 720 分钟之间".to_string(),
        ));
    }
    if scheduled_label.trim().is_empty() {
        return Err(GrowthError::Validation("安排时间不能为空".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bookway_api::{
        ActionRecurrenceDto, ActionRecurrenceFrequencyDto, ActionStateDto, CompanionModeDto,
        CreateJourneyStageRequest, EntryMoodDto, JourneyTypeDto, KnowledgeResourceKindDto,
        KnowledgeResourceStatusDto, NotificationKindDto, PushProviderDto, ReviewAdjustmentKindDto,
        WeekdayDto,
    };

    use super::*;
    use crate::api::GrowthDomainDto;
    use crate::{conf::Config, datasource::MemoryGrowthRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            },
            Arc::new(MemoryGrowthRepository::seeded()),
        )
    }

    #[tokio::test]
    async fn creates_a_journey_with_its_first_action() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "user-a",
                CreateJourneyRequest {
                    title: "学习摄影".to_string(),
                    intent: "记录旅行".to_string(),
                    domain: GrowthDomainDto::Learning,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "3 周".to_string(),
                    first_action_title: "理解曝光三要素".to_string(),
                    first_action_detail: "完成一组对比照片".to_string(),
                    estimated_minutes: 25,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("journey should be created");

        assert_eq!(journey.title, "学习摄影");
        assert_eq!(
            domain
                .today_for("user-a", None, None)
                .await
                .expect("today should load")
                .total,
            1
        );
    }

    #[tokio::test]
    async fn connects_actions_to_stages_and_materializes_the_next_repeat() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "repeat-user",
                CreateJourneyRequest {
                    title: "晨间跑步恢复计划".to_string(),
                    intent: "用低压力节奏重新开始跑步".to_string(),
                    domain: GrowthDomainDto::Movement,
                    journey_type: JourneyTypeDto::Habit,
                    completion_criteria: "每周完成三次轻松跑".to_string(),
                    stages: vec![
                        CreateJourneyStageRequest {
                            title: "恢复节奏".to_string(),
                            detail: "先保持轻松和可恢复".to_string(),
                            completion_criteria: "完成三次轻松跑".to_string(),
                        },
                        CreateJourneyStageRequest {
                            title: "稳定提升".to_string(),
                            detail: String::new(),
                            completion_criteria: "连续两周按自己的节奏完成".to_string(),
                        },
                    ],
                    duration_label: "4 周".to_string(),
                    first_action_title: "轻松跑 20 分钟".to_string(),
                    first_action_detail: "保持可以自然说话的配速".to_string(),
                    estimated_minutes: 20,
                    first_action_scheduled_label: Some("2032-06-18 07:00".to_string()),
                    first_action_scheduled_for: Some("2032-06-18T07:00:00+08:00".to_string()),
                    first_action_scheduled_timezone: Some("Asia/Shanghai".to_string()),
                    first_action_stage_index: Some(0),
                    first_action_recurrence: Some(ActionRecurrenceDto {
                        frequency: ActionRecurrenceFrequencyDto::Weekly,
                        interval: 1,
                        weekdays: vec![WeekdayDto::Monday, WeekdayDto::Thursday],
                        ends_on: Some("2032-06-30".to_string()),
                        anchor_date: None,
                    }),
                },
            )
            .await
            .expect("journey should be created");
        assert_eq!(journey.journey_type, JourneyTypeDto::Habit);
        assert_eq!(journey.stages.len(), 2);
        let first = domain
            .get_journey("repeat-user", &journey.id)
            .await
            .expect("journey should load")
            .actions
            .into_iter()
            .next()
            .expect("first action should exist");
        assert_eq!(
            first.stage_id.as_deref(),
            Some(journey.stages[0].id.as_str())
        );

        domain
            .complete_action("repeat-user", &first.id)
            .await
            .expect("recurring action should complete");
        let detail = domain
            .get_journey("repeat-user", &journey.id)
            .await
            .expect("journey should load");
        assert_eq!(detail.actions.len(), 2);
        let successor = detail
            .actions
            .iter()
            .find(|action| action.id != first.id)
            .expect("next occurrence should be created");
        assert_eq!(successor.state, ActionStateDto::Pending);
        assert_eq!(successor.stage_id, first.stage_id);
        assert_eq!(
            successor.scheduled_for.as_deref(),
            Some("2032-06-21T07:00:00+08:00")
        );
        assert_eq!(
            successor
                .recurrence
                .as_ref()
                .and_then(|recurrence| recurrence.anchor_date.as_deref()),
            Some("2032-06-18")
        );
    }

    #[tokio::test]
    async fn review_exposes_a_user_controlled_smaller_next_step() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "review-adjust-user",
                CreateJourneyRequest {
                    title: "重新开始写作".to_string(),
                    intent: "让写作回到一周安排里".to_string(),
                    domain: GrowthDomainDto::Learning,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: "完成四篇短文".to_string(),
                    stages: Vec::new(),
                    duration_label: "4 周".to_string(),
                    first_action_title: "写 500 字".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 45,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("journey should be created");
        let first = domain
            .get_journey("review-adjust-user", &journey.id)
            .await
            .expect("journey should load")
            .actions
            .into_iter()
            .next()
            .expect("first action should exist");
        domain
            .update_action(
                "review-adjust-user",
                &first.id,
                UpdateActionRequest {
                    state: Some(ActionStateDto::Skipped),
                    ..Default::default()
                },
            )
            .await
            .expect("action should be skippable");
        let follow_up = domain
            .create_action(
                "review-adjust-user",
                CreateActionRequest {
                    journey_id: journey.id,
                    stage_id: None,
                    title: "修改一段".to_string(),
                    detail: String::new(),
                    estimated_minutes: 30,
                    scheduled_label: "本周".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                },
            )
            .await
            .expect("follow-up action should be created");

        let review = domain
            .weekly_review("review-adjust-user")
            .await
            .expect("review should load");
        let suggestion = review
            .adjustment_suggestions
            .iter()
            .find(|suggestion| suggestion.kind == ReviewAdjustmentKindDto::ReduceActionDuration)
            .expect("review should offer a smaller next step");
        assert_eq!(
            suggestion
                .action_patch
                .as_ref()
                .map(|patch| patch.action_id.as_str()),
            Some(follow_up.id.as_str())
        );
        assert_eq!(
            suggestion
                .action_patch
                .as_ref()
                .and_then(|patch| patch.estimated_minutes),
            Some(10)
        );
    }

    #[tokio::test]
    async fn route_journey_retries_reuse_the_same_private_journey() {
        let domain = domain();
        let request = CreateJourneyRequest {
            title: "四周写作练习".to_string(),
            intent: "建立稳定节奏".to_string(),
            domain: GrowthDomainDto::Learning,
            journey_type: JourneyTypeDto::Project,
            completion_criteria: String::new(),
            stages: Vec::new(),
            duration_label: "4 周".to_string(),
            first_action_title: "写 100 字".to_string(),
            first_action_detail: String::new(),
            estimated_minutes: 10,
            first_action_scheduled_label: None,
            first_action_scheduled_for: None,
            first_action_scheduled_timezone: None,
            first_action_stage_index: None,
            first_action_recurrence: None,
        };
        let (first, retry) = tokio::join!(
            domain.create_route_journey("user-a", "route-a", request.clone()),
            domain.create_route_journey("user-a", "route-a", request.clone()),
        );
        let first = first.expect("first route join");
        let retry = retry.expect("concurrent route join retry");
        let other_user = domain
            .create_route_journey("user-b", "route-a", request)
            .await
            .expect("other user route join");
        let after_source_edit = domain
            .create_route_journey(
                "user-a",
                "route-a",
                CreateJourneyRequest {
                    title: "来源内容更新后的标题".to_string(),
                    intent: String::new(),
                    domain: GrowthDomainDto::Leisure,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "8 周".to_string(),
                    first_action_title: "新行动".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 30,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("retry after source edit");

        assert_eq!(first.id, retry.id);
        assert_eq!(first.id, after_source_edit.id);
        assert_eq!(first.title, after_source_edit.title);
        assert_ne!(first.id, other_user.id);
        assert_eq!(
            domain
                .list_journeys("user-a")
                .await
                .expect("journeys")
                .len(),
            1
        );
        assert_eq!(
            domain
                .today_for("user-a", None, None)
                .await
                .expect("today")
                .total,
            1
        );

        let left = domain
            .set_route_participation_intent("user-a", "route-a", false, Some(first.id.clone()))
            .await
            .expect("leave intent");
        let left_retry = domain
            .set_route_participation_intent("user-a", "route-a", false, None)
            .await
            .expect("idempotent leave intent");
        let rejoined = domain
            .set_route_participation_intent("user-a", "route-a", true, Some(first.id.clone()))
            .await
            .expect("rejoin intent");

        assert!(!left.desired_active);
        assert_eq!(left.private_journey_id, None);
        assert_eq!(left.version, left_retry.version);
        assert_eq!(rejoined.version, left.version + 1);
        assert_eq!(rejoined.private_journey_id, Some(first.id));
    }

    #[tokio::test]
    async fn memory_storage_keeps_journeys_and_actions_isolated_by_user() {
        let domain = domain();

        assert!(
            domain
                .list_journeys("another-user")
                .await
                .expect("journeys should load")
                .is_empty()
        );
        assert!(
            domain
                .today_for("another-user", None, None)
                .await
                .expect("today should load")
                .actions
                .is_empty()
        );
        assert!(
            domain
                .complete_action("another-user", "action-stretch")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_an_empty_title() {
        let domain = domain();
        let result = domain
            .create_journey(
                "user-a",
                CreateJourneyRequest {
                    title: " ".to_string(),
                    intent: String::new(),
                    domain: GrowthDomainDto::Learning,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "1 周".to_string(),
                    first_action_title: "开始".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 10,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await;

        assert!(matches!(result, Err(GrowthError::Validation(_))));
    }

    #[tokio::test]
    async fn persists_reminder_preferences_with_a_quiet_window() {
        let domain = domain();

        let preferences = domain
            .update_reminder_preferences(
                "user-a",
                UpdateReminderPreferencesRequest {
                    enabled: true,
                    lead_minutes: 15,
                    timezone: "Asia/Shanghai".to_string(),
                    quiet_hours_start: Some("22:30".to_string()),
                    quiet_hours_end: Some("07:30".to_string()),
                },
            )
            .await
            .expect("preferences should save");

        assert!(preferences.enabled);
        assert_eq!(preferences.lead_minutes, 15);
        assert_eq!(preferences.timezone, "Asia/Shanghai");
        assert_eq!(preferences.quiet_hours_start.as_deref(), Some("22:30"));
        assert_eq!(
            domain
                .reminder_preferences("user-a")
                .await
                .expect("preferences should load"),
            preferences
        );
    }

    #[tokio::test]
    async fn rejects_an_incomplete_or_empty_reminder_quiet_window() {
        let domain = domain();
        let incomplete = domain
            .update_reminder_preferences(
                "user-a",
                UpdateReminderPreferencesRequest {
                    enabled: true,
                    lead_minutes: 0,
                    timezone: "Asia/Shanghai".to_string(),
                    quiet_hours_start: Some("22:00".to_string()),
                    quiet_hours_end: None,
                },
            )
            .await;
        let empty = domain
            .update_reminder_preferences(
                "user-a",
                UpdateReminderPreferencesRequest {
                    enabled: true,
                    lead_minutes: 0,
                    timezone: "Asia/Shanghai".to_string(),
                    quiet_hours_start: Some("22:00".to_string()),
                    quiet_hours_end: Some("22:00".to_string()),
                },
            )
            .await;

        assert!(matches!(incomplete, Err(GrowthError::Validation(_))));
        assert!(matches!(empty, Err(GrowthError::Validation(_))));
    }

    #[tokio::test]
    async fn registers_and_revokes_a_push_device_without_returning_its_endpoint() {
        let domain = domain();
        let device = domain
            .register_push_device(
                "user-a",
                RegisterPushDeviceRequest {
                    device_id: "ios-installation-1".to_string(),
                    provider: PushProviderDto::Expo,
                    endpoint: "ExponentPushToken[opaque]".to_string(),
                },
            )
            .await
            .expect("device should register");

        assert_eq!(device.device_id, "ios-installation-1");
        assert_eq!(device.provider, PushProviderDto::Expo);
        assert!(device.active);
        domain
            .revoke_push_device("user-a", &device.device_id)
            .await
            .expect("device should revoke");
    }

    #[tokio::test]
    async fn manages_route_and_action_lifecycle() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "user-a",
                CreateJourneyRequest {
                    title: "四周写作练习".to_string(),
                    intent: "留下可回看的作品".to_string(),
                    domain: GrowthDomainDto::Learning,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "4 周".to_string(),
                    first_action_title: "写 100 字".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 10,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("journey should be created");

        let detail = domain
            .get_journey("user-a", &journey.id)
            .await
            .expect("journey detail should load");
        assert_eq!(detail.actions.len(), 1);

        let action = domain
            .create_action(
                "user-a",
                CreateActionRequest {
                    journey_id: journey.id.clone(),
                    stage_id: None,
                    title: "修改开头".to_string(),
                    detail: "让第一段更具体".to_string(),
                    estimated_minutes: 15,
                    scheduled_label: "明天".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                },
            )
            .await
            .expect("action should be created");
        let updated = domain
            .update_action(
                "user-a",
                &action.id,
                UpdateActionRequest {
                    state: Some(ActionStateDto::Skipped),
                    ..Default::default()
                },
            )
            .await
            .expect("action should update");
        assert_eq!(updated.state, ActionStateDto::Skipped);

        let paused = domain
            .update_journey(
                "user-a",
                &journey.id,
                UpdateJourneyRequest {
                    status: Some(JourneyStatusDto::Paused),
                    ..Default::default()
                },
            )
            .await
            .expect("journey should update");
        assert_eq!(paused.status, JourneyStatusDto::Paused);
    }

    #[tokio::test]
    async fn persists_entries_and_builds_a_review_from_real_actions() {
        let domain = domain();
        let entry = domain
            .create_entry(
                "demo-user",
                CreateGrowthEntryRequest {
                    action_id: Some("action-stretch".to_string()),
                    journey_id: Some("journey-running".to_string()),
                    body: "跑后身体很放松".to_string(),
                    mood: EntryMoodDto::Calm,
                    duration_minutes: Some(8),
                    quantity: None,
                    location: None,
                    photo_url: None,
                    published: false,
                },
            )
            .await
            .expect("entry should persist");

        assert_eq!(
            domain
                .list_entries("demo-user")
                .await
                .expect("entries should load"),
            vec![entry]
        );
        assert!(
            domain
                .list_entries("another-user")
                .await
                .expect("other entries should load")
                .is_empty()
        );
        let review = domain
            .weekly_review("demo-user")
            .await
            .expect("review should load");
        assert_eq!(review.completed_actions, 1);
        assert_eq!(review.focus_minutes, 8);
        assert_eq!(review.entry_count, 1);
    }

    #[tokio::test]
    async fn companion_offers_a_small_recovery_step_without_changing_the_plan() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "recovering-user",
                CreateJourneyRequest {
                    title: "恢复阅读节奏".to_string(),
                    intent: "重新建立低压力阅读习惯".to_string(),
                    domain: GrowthDomainDto::Learning,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "4 周".to_string(),
                    first_action_title: "阅读 30 分钟".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 30,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("journey should be created");
        let first_action = domain
            .get_journey("recovering-user", &journey.id)
            .await
            .expect("journey should load")
            .actions
            .into_iter()
            .next()
            .expect("first action should exist");
        domain
            .update_action(
                "recovering-user",
                &first_action.id,
                UpdateActionRequest {
                    state: Some(ActionStateDto::Skipped),
                    ..Default::default()
                },
            )
            .await
            .expect("action should be skippable");
        let pending = domain
            .create_action(
                "recovering-user",
                CreateActionRequest {
                    journey_id: journey.id,
                    stage_id: None,
                    title: "读两页".to_string(),
                    detail: String::new(),
                    estimated_minutes: 24,
                    scheduled_label: "今晚".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                },
            )
            .await
            .expect("next action should be created");

        let brief = domain
            .companion_brief_for("recovering-user", None, None)
            .await
            .expect("companion brief should load");

        assert_eq!(brief.mode, CompanionModeDto::StartSmall);
        assert_eq!(brief.suggested_minutes, Some(8));
        assert_eq!(
            brief.suggested_action.as_ref().map(|action| &action.id),
            Some(&pending.id)
        );
        assert_eq!(
            pending.estimated_minutes, 24,
            "the companion must not mutate plans"
        );
    }

    #[tokio::test]
    async fn schedules_actions_for_the_requested_local_day() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "scheduled-user",
                CreateJourneyRequest {
                    title: "傍晚散步".to_string(),
                    intent: "把散步留给下班后的自己".to_string(),
                    domain: GrowthDomainDto::Wellness,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "长期".to_string(),
                    first_action_title: "绕街区走一圈".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 20,
                    first_action_scheduled_label: Some("明天 18:30".to_string()),
                    first_action_scheduled_for: Some("2032-06-18T18:30:00+08:00".to_string()),
                    first_action_scheduled_timezone: Some("Asia/Shanghai".to_string()),
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("journey should be created");

        let scheduled = domain
            .today_for("scheduled-user", Some("2032-06-18"), Some("Asia/Shanghai"))
            .await
            .expect("scheduled local day should load");
        assert_eq!(scheduled.total, 1);
        assert_eq!(scheduled.actions[0].journey_id, journey.id);
        assert_eq!(
            scheduled.actions[0].scheduled_for.as_deref(),
            Some("2032-06-18T18:30:00+08:00")
        );
        domain
            .update_action(
                "scheduled-user",
                &scheduled.actions[0].id,
                UpdateActionRequest {
                    scheduled_label: Some("后天 20:00".to_string()),
                    scheduled_for: Some("2032-06-20T20:00:00+08:00".to_string()),
                    scheduled_timezone: Some("Asia/Shanghai".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("action should be rescheduled");
        assert!(
            domain
                .today_for("scheduled-user", Some("2032-06-18"), Some("Asia/Shanghai"))
                .await
                .expect("previous local day should load")
                .actions
                .is_empty()
        );
        assert_eq!(
            domain
                .today_for("scheduled-user", Some("2032-06-20"), Some("Asia/Shanghai"))
                .await
                .expect("rescheduled local day should load")
                .total,
            1
        );

        let invalid = domain
            .create_action(
                "scheduled-user",
                CreateActionRequest {
                    journey_id: journey.id,
                    stage_id: None,
                    title: "不完整安排".to_string(),
                    detail: String::new(),
                    estimated_minutes: 10,
                    scheduled_label: "稍后".to_string(),
                    scheduled_for: Some("2032-06-18T20:00:00+08:00".to_string()),
                    scheduled_timezone: None,
                    recurrence: None,
                },
            )
            .await;
        assert!(matches!(invalid, Err(GrowthError::Validation(_))));
    }

    #[tokio::test]
    async fn companion_recovers_an_overdue_scheduled_action_without_mutating_it() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "overdue-user",
                CreateJourneyRequest {
                    title: "重新建立写作节奏".to_string(),
                    intent: "把写作放回每周安排".to_string(),
                    domain: GrowthDomainDto::Learning,
                    journey_type: JourneyTypeDto::Project,
                    completion_criteria: String::new(),
                    stages: Vec::new(),
                    duration_label: "4 周".to_string(),
                    first_action_title: "写一个段落".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 24,
                    first_action_scheduled_label: Some("上周三 19:00".to_string()),
                    first_action_scheduled_for: Some("2020-01-01T19:00:00+08:00".to_string()),
                    first_action_scheduled_timezone: Some("Asia/Shanghai".to_string()),
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await
            .expect("journey should be created");
        let action = domain
            .get_journey("overdue-user", &journey.id)
            .await
            .expect("journey should load")
            .actions
            .into_iter()
            .next()
            .expect("action should exist");

        let brief = domain
            .companion_brief_for("overdue-user", Some("2032-06-18"), Some("Asia/Shanghai"))
            .await
            .expect("companion should load");

        assert_eq!(brief.mode, CompanionModeDto::StartSmall);
        assert_eq!(brief.suggested_minutes, Some(8));
        assert_eq!(
            brief
                .suggested_action
                .as_ref()
                .map(|suggested| &suggested.id),
            Some(&action.id)
        );
        assert_eq!(
            domain
                .get_journey("overdue-user", &journey.id)
                .await
                .expect("journey should still load")
                .actions[0]
                .estimated_minutes,
            24,
            "the companion must never mutate an overdue action"
        );
    }

    #[tokio::test]
    async fn companion_does_not_pressure_a_user_without_active_routes() {
        let brief = domain()
            .companion_brief_for("new-user", None, None)
            .await
            .expect("companion brief should load");

        assert_eq!(brief.mode, CompanionModeDto::PlanNext);
        assert!(brief.suggested_action.is_none());
        assert_eq!(brief.active_journeys, 0);
    }

    #[tokio::test]
    async fn persists_searches_and_updates_private_knowledge_resources() {
        let domain = domain();
        let request = CreateKnowledgeResourceRequest {
            title: " 看不见的城市 ".to_string(),
            creator: "伊塔洛·卡尔维诺".to_string(),
            summary: "关于城市、记忆与欲望".to_string(),
            kind: KnowledgeResourceKindDto::Book,
            status: KnowledgeResourceStatusDto::Active,
            source_url: None,
            body: Some("第一章\n城市与记忆".to_string()),
            tags: vec!["城市".to_string(), " 文学 ".to_string()],
            journey_id: Some("journey-reading".to_string()),
        };
        let resource = domain
            .create_knowledge(
                "demo-user",
                request.clone(),
                Some("knowledge-create-1".to_string()),
            )
            .await
            .expect("knowledge resource should persist");
        let retried = domain
            .create_knowledge("demo-user", request, Some("knowledge-create-1".to_string()))
            .await
            .expect("idempotent retry should return the original resource");
        assert_eq!(retried.id, resource.id);
        assert_eq!(resource.tags, vec!["城市", "文学"]);
        assert!(
            domain
                .list_knowledge("another-user", KnowledgeQueryRequest::default(),)
                .await
                .expect("another user's resources should load")
                .is_empty()
        );
        let matches = domain
            .list_knowledge(
                "demo-user",
                KnowledgeQueryRequest {
                    q: Some("记忆".to_string()),
                    kind: Some(KnowledgeResourceKindDto::Book),
                    ..Default::default()
                },
            )
            .await
            .expect("knowledge search should load");
        assert_eq!(matches.len(), 1);
        let updated = domain
            .update_knowledge(
                "demo-user",
                &resource.id,
                UpdateKnowledgeResourceRequest {
                    progress: Some(100),
                    current_position: Some(2),
                    reading_seconds: Some(900),
                    bookmarks: Some(vec!["城市与记忆".to_string()]),
                    last_opened_at: Some("2026-08-15T08:00:00Z".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("knowledge resource should update");
        assert_eq!(updated.progress, 100);
        assert_eq!(updated.status, KnowledgeResourceStatusDto::Completed);
        assert_eq!(updated.bookmarks, vec!["城市与记忆"]);
    }

    #[tokio::test]
    async fn rejects_knowledge_idempotency_conflicts_and_foreign_routes() {
        let domain = domain();
        let request = CreateKnowledgeResourceRequest {
            title: "第一本书".to_string(),
            creator: String::new(),
            summary: String::new(),
            kind: KnowledgeResourceKindDto::Book,
            status: KnowledgeResourceStatusDto::Inbox,
            source_url: None,
            body: None,
            tags: Vec::new(),
            journey_id: None,
        };
        domain
            .create_knowledge("demo-user", request.clone(), Some("same-key".to_string()))
            .await
            .expect("first create should work");
        let mut changed = request;
        changed.title = "另一本书".to_string();
        assert!(matches!(
            domain
                .create_knowledge("demo-user", changed, Some("same-key".to_string()))
                .await,
            Err(GrowthError::Repository(
                crate::datasource::RepositoryError::IdempotencyConflict
            ))
        ));
        let foreign_route = CreateKnowledgeResourceRequest {
            title: "越权资源".to_string(),
            creator: String::new(),
            summary: String::new(),
            kind: KnowledgeResourceKindDto::Note,
            status: KnowledgeResourceStatusDto::Inbox,
            source_url: None,
            body: None,
            tags: Vec::new(),
            journey_id: Some("journey-reading".to_string()),
        };
        assert!(
            domain
                .create_knowledge("another-user", foreign_route, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn exposes_a_private_notification_inbox_and_marks_items_read_idempotently() {
        let domain = domain();
        let inbox = domain
            .list_notifications("demo-user", NotificationQueryRequest::default())
            .await
            .expect("seeded inbox should load");
        assert_eq!(inbox.items.len(), 1);
        assert_eq!(inbox.unread_count, 1);

        let notification_id = inbox.items[0].id.clone();
        let read = domain
            .mark_notification_read("demo-user", &notification_id)
            .await
            .expect("the owner can mark the notification read");
        assert!(read.read_at.is_some());

        let retry = domain
            .mark_notification_read("demo-user", &notification_id)
            .await
            .expect("marking a read notification should be safe to retry");
        assert_eq!(retry.read_at, read.read_at);
        assert_eq!(
            domain
                .list_notifications("demo-user", NotificationQueryRequest::default())
                .await
                .expect("inbox should reload")
                .unread_count,
            0
        );
        assert!(
            domain
                .mark_notification_read("another-user", &notification_id)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn creates_deduplicated_notifications_without_cross_user_access() {
        let domain = domain();
        let request = CreateUserNotificationRequest {
            kind: NotificationKindDto::Community,
            source_id: "like:reader:post-city".to_string(),
            title: "收到一个赞".to_string(),
            body: "有人赞了你的内容".to_string(),
            data: serde_json::json!({
                "post_id": "post-city",
                "actor_id": "reader",
            }),
        };

        let created = domain
            .create_notification("author", request.clone())
            .await
            .expect("first producer write should work");
        let retry = domain
            .create_notification("author", request.clone())
            .await
            .expect("retries should reuse the first notification");
        assert_eq!(retry.id, created.id);
        assert_eq!(
            domain
                .list_notifications("author", NotificationQueryRequest::default())
                .await
                .expect("recipient inbox should load")
                .items
                .len(),
            1
        );
        assert!(
            domain
                .list_notifications("reader", NotificationQueryRequest::default())
                .await
                .expect("other inbox should load")
                .items
                .is_empty()
        );
        assert!(matches!(
            domain.create_notification("reader", request).await,
            Err(GrowthError::Repository(
                crate::datasource::RepositoryError::NotificationSourceConflict(_)
            ))
        ));
    }
}
