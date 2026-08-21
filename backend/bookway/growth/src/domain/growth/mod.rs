use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};
use url::Url;
use uuid::Uuid;

use crate::api::pb;
use crate::domain::{Domain, GrowthError};

impl Domain {
    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::Journey>, GrowthError> {
        Ok(self.Dao.list_journeys(user_id).await?)
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        mut request: pb::CreateJourneyRequest,
    ) -> Result<pb::Journey, GrowthError> {
        let idempotency_key = normalize_idempotency_key(request.idempotency_key.take())?;
        let (journey, first_action) = build_journey(request)?;
        Ok(self
            .Dao
            .create_journey(user_id, journey, first_action, idempotency_key)
            .await?)
    }

    pub(crate) async fn create_route_journey(
        &self,
        user_id: &str,
        source_route_id: &str,
        request: pb::CreateJourneyRequest,
        additional_actions: Vec<pb::RouteActionTemplate>,
    ) -> Result<pb::Journey, GrowthError> {
        validate_identifier("来源路线 ID", source_route_id)?;
        let (journey, first_action) = build_journey(request)?;
        let mut actions = vec![first_action];
        actions.extend(build_route_actions(
            &journey.id,
            &journey.stages,
            additional_actions,
        )?);
        Ok(self
            .Dao
            .create_route_journey(user_id, source_route_id, journey, actions)
            .await?)
    }

    pub(crate) async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
    ) -> Result<pb::RouteParticipationIntent, GrowthError> {
        validate_identifier("路线 ID", route_id)?;
        if let Some(journey_id) = private_journey_id.as_deref() {
            validate_identifier("私人路线 ID", journey_id)?;
        }
        Ok(self
            .Dao
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
    ) -> Result<pb::JourneyDetail, GrowthError> {
        Ok(self.Dao.get_journey(user_id, journey_id).await?)
    }

    pub(crate) async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: pb::UpdateJourneyRequest,
    ) -> Result<pb::Journey, GrowthError> {
        validate_journey_update(&request)?;
        Ok(self
            .Dao
            .update_journey(user_id, journey_id, request)
            .await?)
    }

    pub(crate) async fn create_action(
        &self,
        user_id: &str,
        mut request: pb::CreateActionRequest,
    ) -> Result<pb::Action, GrowthError> {
        let idempotency_key = normalize_idempotency_key(request.idempotency_key.take())?;
        validate_action(
            &request.title,
            request.estimated_minutes,
            &request.scheduled_label,
        )?;
        let (scheduled_for, scheduled_timezone) =
            normalize_schedule(request.scheduled_for, request.scheduled_timezone)?;
        let journey = self
            .Dao
            .get_journey(user_id, &request.journey_id)
            .await?;
        let stages = journey
            .journey
            .as_ref()
            .map(|journey| journey.stages.as_slice())
            .ok_or_else(|| GrowthError::Validation("路线数据不完整".to_string()))?;
        validate_action_stage(request.stage_id.as_deref(), stages)?;
        request.recurrence = normalize_recurrence(
            request.recurrence,
            scheduled_for.as_deref(),
            scheduled_timezone.as_deref(),
        )?;
        let action = pb::Action {
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
            state: pb::ActionState::Pending as i32,
        };
        Ok(self
            .Dao
            .create_action(user_id, action, idempotency_key)
            .await?)
    }

    pub(crate) async fn today_for(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<pb::TodaySummary, GrowthError> {
        let schedule_context = schedule_context(local_date, timezone)?;
        let actions = self
            .Dao
            .today(user_id, schedule_context.local_date)
            .await?;
        let completed = actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Completed as i32)
            .count();
        let focus_minutes = actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Completed as i32)
            .map(|action| action.estimated_minutes)
            .sum();
        Ok(pb::TodaySummary {
            completed: count_u32(completed),
            total: count_u32(actions.len()),
            focus_minutes,
            actions,
        })
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<pb::CompleteActionResponse, GrowthError> {
        let action = self.Dao.complete_action(user_id, action_id).await?;
        let source_route_id = self
            .Dao
            .source_route_id_for_journey(user_id, &action.journey_id)
            .await?;
        let source_knowledge_content_id = if source_route_id.is_none() {
            self.Dao
                .source_knowledge_content_id_for_journey(user_id, &action.journey_id)
                .await?
        } else {
            None
        };
        Ok(pb::CompleteActionResponse {
            action: Some(action),
            source_route_id,
            source_knowledge_content_id,
        })
    }

    pub(crate) async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        mut request: pb::UpdateActionRequest,
    ) -> Result<pb::Action, GrowthError> {
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
        if request.state == Some(pb::ActionState::Completed as i32) {
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
            .Dao
            .update_action(user_id, action_id, request)
            .await?)
    }

    pub(crate) async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<pb::ReminderPreference, GrowthError> {
        Ok(self.Dao.reminder_preferences(user_id).await?)
    }

    pub(crate) async fn update_reminder_preferences(
        &self,
        user_id: &str,
        mut request: pb::UpdateReminderPreferencesRequest,
    ) -> Result<pb::ReminderPreference, GrowthError> {
        validate_reminder_preferences(&request)?;
        request.timezone = request.timezone.trim().to_string();
        request.quiet_hours_start = request
            .quiet_hours_start
            .map(|value| value.trim().to_string());
        request.quiet_hours_end = request
            .quiet_hours_end
            .map(|value| value.trim().to_string());
        Ok(self
            .Dao
            .update_reminder_preferences(user_id, request)
            .await?)
    }

    pub(crate) async fn register_push_device(
        &self,
        user_id: &str,
        mut request: pb::RegisterPushDeviceRequest,
    ) -> Result<pb::PushDevice, GrowthError> {
        validate_identifier("设备 ID", &request.device_id)?;
        validate_text(&request.endpoint, 1, 4_096, "推送地址")?;
        request.device_id = request.device_id.trim().to_string();
        request.endpoint = request.endpoint.trim().to_string();
        Ok(self
            .Dao
            .register_push_device(user_id, request)
            .await?)
    }

    pub(crate) async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), GrowthError> {
        validate_identifier("设备 ID", device_id)?;
        self.Dao
            .revoke_push_device(user_id, device_id)
            .await?;
        Ok(())
    }

    pub(crate) async fn list_notifications(
        &self,
        user_id: &str,
        mut request: pb::NotificationQueryRequest,
    ) -> Result<pb::NotificationPage, GrowthError> {
        request.cursor = normalize_notification_cursor(request.cursor)?;
        Ok(self.Dao.list_notifications(user_id, request).await?)
    }

    pub(crate) async fn create_notification(
        &self,
        user_id: &str,
        mut request: pb::CreateNotificationRequest,
    ) -> Result<pb::UserNotification, GrowthError> {
        validate_identifier("通知接收者 ID", user_id)?;
        validate_notification(&request)?;
        request.source_id = request.source_id.trim().to_string();
        request.title = request.title.trim().to_string();
        request.body = request.body.trim().to_string();
        Ok(self
            .Dao
            .create_notification(user_id, request)
            .await?)
    }

    pub(crate) async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<pb::UserNotification, GrowthError> {
        Uuid::parse_str(notification_id)
            .map_err(|_| GrowthError::Validation("通知 ID 格式不正确".to_string()))?;
        Ok(self
            .Dao
            .mark_notification_read(user_id, notification_id)
            .await?)
    }

    pub(crate) async fn list_entries(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::GrowthEntry>, GrowthError> {
        Ok(self.Dao.list_entries(user_id).await?)
    }

    pub(crate) async fn create_entry(
        &self,
        user_id: &str,
        mut request: pb::CreateEntryRequest,
    ) -> Result<pb::GrowthEntry, GrowthError> {
        let idempotency_key = normalize_idempotency_key(request.idempotency_key.take())?;
        validate_entry(&request)?;
        let publication_status = if request.published {
            pb::EntryPublicationStatus::Pending as i32
        } else {
            pb::EntryPublicationStatus::Private as i32
        };
        let entry = pb::GrowthEntry {
            id: Uuid::now_v7().to_string(),
            action_id: request.action_id,
            journey_id: request.journey_id,
            body: request.body.trim().to_string(),
            mood: request.mood,
            duration_minutes: request.duration_minutes,
            quantity: trimmed_option(request.quantity),
            location: trimmed_option(request.location),
            // Raw URLs cannot safely be carried into the public travelogue
            // pipeline. A Media asset ID is validated by BBS Link at attach
            // time with the entry author's identity.
            photo_url: None,
            photo_media_id: trimmed_option(request.photo_media_id),
            created_at: now_rfc3339(),
            // A public request is only intent. The durable publication worker
            // changes this after content review returns a terminal status.
            published: false,
            publication_status,
            public_content_id: None,
            publication_error: None,
        };
        Ok(self
            .Dao
            .create_entry(user_id, entry, idempotency_key)
            .await?)
    }

    pub(crate) async fn retry_entry_publication(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<pb::GrowthEntry, GrowthError> {
        Uuid::parse_str(entry_id)
            .map_err(|_| GrowthError::Validation("记录 ID 格式不正确".to_string()))?;
        Ok(self
            .Dao
            .retry_entry_publication(user_id, entry_id)
            .await?)
    }

    pub(crate) async fn weekly_review(
        &self,
        user_id: &str,
    ) -> Result<pb::WeeklyReviewSummary, GrowthError> {
        let snapshot = self.Dao.review_snapshot(user_id).await?;
        let completed_actions = snapshot
            .actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Completed as i32)
            .count();
        let skipped_actions = snapshot
            .actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Skipped as i32)
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
            .sum();
        let focus_minutes = if recorded_minutes > 0 {
            recorded_minutes
        } else {
            snapshot
                .actions
                .iter()
                .filter(|action| action.state == pb::ActionState::Completed as i32)
                .map(|action| action.estimated_minutes)
                .sum()
        };
        let journey_domains = snapshot
            .journeys
            .iter()
            .map(|journey| (journey.id.as_str(), journey.domain))
            .collect::<std::collections::HashMap<_, _>>();
        let mut domains = std::collections::HashMap::<i32, (usize, usize)>::new();
        for action in &snapshot.actions {
            let Some(domain) = journey_domains.get(action.journey_id.as_str()) else {
                continue;
            };
            let counts = domains.entry(*domain).or_default();
            counts.1 += 1;
            counts.0 += usize::from(action.state == pb::ActionState::Completed as i32);
        }
        let mut domains = domains
            .into_iter()
            .map(
                |(domain, (completed_actions, total_actions))| pb::ReviewDomainProgress {
                    domain,
                    completed_actions: count_u32(completed_actions),
                    total_actions: count_u32(total_actions),
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
        Ok(pb::WeeklyReviewSummary {
            period_start: week_start.to_string(),
            period_end: now.date().to_string(),
            completed_actions: count_u32(completed_actions),
            skipped_actions: count_u32(skipped_actions),
            focus_minutes,
            entry_count: count_u32(snapshot.entries.len()),
            active_journeys: snapshot
                .journeys
                .iter()
                .filter(|journey| journey.status == pb::JourneyStatus::Active as i32)
                .count()
                .try_into()
                .unwrap_or(u32::MAX),
            completion_rate,
            domains,
            reflection_prompts: reflection_prompts(completion_rate, snapshot.entries.is_empty()),
            adjustment_suggestions,
        })
    }

    pub(crate) async fn save_weekly_review(
        &self,
        user_id: &str,
        request: pb::SaveWeeklyReviewRequest,
    ) -> Result<pb::ReviewRecord, GrowthError> {
        validate_text(&request.reflection, 1, 4_000, "复盘结论")?;
        validate_text(&request.next_focus, 0, 500, "下周重点")?;
        let summary = self.weekly_review(user_id).await?;
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| GrowthError::Validation(format!("复盘时间无效: {error}")))?;
        let stable_key = format!(
            "bookway:weekly-review:{user_id}:{}:{}",
            summary.period_start, summary.period_end
        );
        let review = pb::ReviewRecord {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
            summary: Some(summary),
            reflection: request.reflection.trim().to_string(),
            next_focus: request.next_focus.trim().to_string(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            applied_adjustments: Vec::new(),
        };
        Ok(self.Dao.save_weekly_review(user_id, review).await?)
    }

    pub(crate) async fn apply_weekly_review_adjustment(
        &self,
        user_id: &str,
        request: pb::ApplyWeeklyReviewAdjustmentRequest,
    ) -> Result<pb::ApplyWeeklyReviewAdjustmentResponse, GrowthError> {
        Uuid::parse_str(request.review_id.trim())
            .map_err(|_| GrowthError::Validation("复盘标识无效".to_string()))?;
        let applied = self
            .Dao
            .apply_weekly_review_adjustment(user_id, &request.review_id, request.suggestion_index)
            .await?;
        Ok(pb::ApplyWeeklyReviewAdjustmentResponse {
            review: Some(applied.review),
            decision: Some(applied.decision),
        })
    }

    pub(crate) async fn companion_brief_for(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<pb::CompanionBrief, GrowthError> {
        let schedule_context = schedule_context(local_date, timezone)?;
        let (actions, snapshot) = tokio::try_join!(
            self.Dao.today(user_id, schedule_context.local_date),
            self.Dao.review_snapshot(user_id),
        )?;
        let active_journey_ids = snapshot
            .journeys
            .iter()
            .filter(|journey| journey.status == pb::JourneyStatus::Active as i32)
            .map(|journey| journey.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let active_journeys = active_journey_ids.len();
        let active_actions = actions
            .iter()
            .filter(|action| active_journey_ids.contains(action.journey_id.as_str()))
            .collect::<Vec<_>>();
        let completed_actions = active_actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Completed as i32)
            .count();
        let skipped_actions = active_actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Skipped as i32)
            .count();
        let overdue_action = snapshot
            .actions
            .iter()
            .filter(|action| active_journey_ids.contains(action.journey_id.as_str()))
            .filter(|action| action.state == pb::ActionState::Pending as i32)
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
                .filter(|action| action.state == pb::ActionState::Pending as i32)
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
                        pb::CompanionMode::StartSmall as i32,
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
                    pb::CompanionMode::StartSmall as i32,
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
                    pb::CompanionMode::StartSmall as i32,
                    format!("先给「{}」{minutes} 分钟", action.title),
                    "今天不必做完所有事。先让最小的一步发生，节奏会重新回来。".to_string(),
                    "它是当前待办中用时最短的一步，适合作为低压力的开始。".to_string(),
                    Some(minutes),
                    "开始前，怎样让环境更支持这一步？".to_string(),
                )
            }
            (Some(action), false) => (
                pb::CompanionMode::KeepGoing as i32,
                format!("下一步是「{}」", action.title),
                "已经走出的每一步都会留下来。按原有节奏继续，或根据状态自行缩小它。".to_string(),
                "你已经完成了今天的一部分行动，因此保留当前路线的下一步。".to_string(),
                Some(action.estimated_minutes),
                "今天已经发生的什么，让接下来的行动更顺一点？".to_string(),
            ),
            (None, _) if completed_actions > 0 => (
                pb::CompanionMode::Celebrate as i32,
                "今天的行动已经告一段落".to_string(),
                "不需要再追赶。若愿意，留下一句感受，让这次完成成为以后能看见的证据。".to_string(),
                "今天没有待办行动，且已有完成记录。".to_string(),
                None,
                "今天哪一个瞬间，最值得被记住？".to_string(),
            ),
            (None, _) if active_journeys > 0 => (
                pb::CompanionMode::PlanNext as i32,
                "今天可以留一点空白".to_string(),
                "你的路线仍在这里。想继续时，为它安排一个足够小、足够具体的下一步即可。"
                    .to_string(),
                "当前没有待办行动，但仍有进行中的路线。".to_string(),
                None,
                "下一次行动，怎样安排才更符合现在的生活节奏？".to_string(),
            ),
            (None, _) => (
                pb::CompanionMode::PlanNext as i32,
                "从一个想靠近的方向开始".to_string(),
                "不必一次规划很远。选择一条想尝试的路线，再为今天留下第一个小行动。".to_string(),
                "你还没有进行中的路线或待办行动。".to_string(),
                None,
                "最近有什么变化，是你愿意花一点时间靠近的？".to_string(),
            ),
        };

        Ok(pb::CompanionBrief {
            mode,
            headline,
            message,
            reason,
            suggested_action: pending_action,
            suggested_minutes,
            completed_actions: count_u32(completed_actions),
            total_actions: count_u32(active_actions.len()),
            active_journeys: count_u32(active_journeys),
            reflection_prompt,
        })
    }

    pub(crate) async fn list_knowledge(
        &self,
        user_id: &str,
        mut query: pb::KnowledgeQueryRequest,
    ) -> Result<Vec<pb::KnowledgeResource>, GrowthError> {
        query.q = normalize_query_filter(query.q, "检索词")?;
        query.tag = normalize_query_filter(query.tag, "标签")?;
        Ok(self.Dao.list_knowledge(user_id, query).await?)
    }

    pub(crate) async fn create_knowledge(
        &self,
        user_id: &str,
        request: pb::CreateKnowledgeRequest,
        idempotency_key: Option<String>,
    ) -> Result<pb::KnowledgeResource, GrowthError> {
        validate_knowledge_create(&request)?;
        let idempotency_key = normalize_idempotency_key(idempotency_key)?;
        let now = now_rfc3339();
        let resource = pb::KnowledgeResource {
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
            source_content_id: trimmed_option(request.source_content_id),
        };
        Ok(self
            .Dao
            .create_knowledge(user_id, resource, idempotency_key)
            .await?)
    }

    pub(crate) async fn start_knowledge_journey(
        &self,
        user_id: &str,
        resource_id: &str,
        request: pb::StartKnowledgeJourneyRequest,
    ) -> Result<pb::KnowledgeJourney, GrowthError> {
        validate_identifier("知识资源 ID", resource_id)?;
        let (journey, first_action) = build_journey(pb::CreateJourneyRequest {
            user_id: user_id.to_string(),
            title: request.title,
            intent: request.intent,
            domain: request.domain,
            journey_type: request.journey_type,
            completion_criteria: request.completion_criteria,
            stages: request.stages,
            duration_label: request.duration_label,
            first_action_title: request.first_action_title,
            first_action_detail: request.first_action_detail,
            estimated_minutes: request.estimated_minutes,
            first_action_scheduled_label: request.first_action_scheduled_label,
            first_action_scheduled_for: request.first_action_scheduled_for,
            first_action_scheduled_timezone: request.first_action_scheduled_timezone,
            first_action_stage_index: request.first_action_stage_index,
            first_action_recurrence: request.first_action_recurrence,
            idempotency_key: None,
        })?;
        Ok(self
            .Dao
            .start_knowledge_journey(user_id, resource_id, journey, first_action)
            .await?)
    }

    pub(crate) async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        mut request: pb::UpdateKnowledgeRequest,
    ) -> Result<pb::KnowledgeResource, GrowthError> {
        validate_knowledge_update(&request)?;
        request.title = request.title.map(|value| value.trim().to_string());
        request.creator = request.creator.map(|value| value.trim().to_string());
        request.summary = request.summary.map(|value| value.trim().to_string());
        request.source_url = request.source_url.map(|value| value.trim().to_string());
        request.body = request.body.map(|value| value.trim().to_string());
        request.tags = request.tags.map(|tags| pb::StringList {
            values: normalize_tags(tags.values),
        });
        request.journey_id = request.journey_id.map(|value| value.trim().to_string());
        request.bookmarks = request.bookmarks.map(|bookmarks| pb::StringList {
            values: bookmarks
                .values
                .into_iter()
                .map(|bookmark| bookmark.trim().to_string())
                .collect(),
        });
        request.last_opened_at = request.last_opened_at.map(|value| value.trim().to_string());
        Ok(self
            .Dao
            .update_knowledge(user_id, resource_id, request)
            .await?)
    }
}

fn build_journey(
    request: pb::CreateJourneyRequest,
) -> Result<(pb::Journey, pb::Action), GrowthError> {
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
        .map(|index| {
            usize::try_from(index)
                .map_err(|_| GrowthError::Validation("首个行动所属阶段不存在".to_string()))
        })
        .transpose()?
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
    let journey = pb::Journey {
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
        status: pb::JourneyStatus::Active as i32,
        progress: 0,
        duration_label: request.duration_label,
        next_action: request.first_action_title.trim().to_string(),
        participant_count: 1,
    };
    let first_action = pb::Action {
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
        state: pb::ActionState::Pending as i32,
    };
    Ok((journey, first_action))
}

fn build_stages(stages: Vec<pb::JourneyStageInput>) -> Result<Vec<pb::JourneyStage>, GrowthError> {
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
            Ok(pb::JourneyStage {
                id: Uuid::now_v7().to_string(),
                title: stage.title.trim().to_string(),
                detail: stage.detail.trim().to_string(),
                completion_criteria: stage.completion_criteria.trim().to_string(),
                position: u32::try_from(position)
                    .map_err(|_| GrowthError::Validation("路线阶段数量无效".to_string()))?,
            })
        })
        .collect()
}

fn build_route_actions(
    journey_id: &str,
    stages: &[pb::JourneyStage],
    templates: Vec<pb::RouteActionTemplate>,
) -> Result<Vec<pb::Action>, GrowthError> {
    if templates.len() > 49 {
        return Err(GrowthError::Validation(
            "路线最多包含 50 个行动".to_string(),
        ));
    }
    templates
        .into_iter()
        .map(|template| {
            validate_action(
                &template.title,
                template.estimated_minutes,
                &template.scheduled_label,
            )?;
            validate_text(&template.detail, 0, 1_000, "路线行动说明")?;
            let stage_id = template
                .stage_index
                .map(|index| {
                    let index = usize::try_from(index)
                        .map_err(|_| GrowthError::Validation("路线行动阶段索引无效".to_string()))?;
                    stages
                        .get(index)
                        .map(|stage| stage.id.clone())
                        .ok_or_else(|| {
                            GrowthError::Validation("路线行动所属阶段不存在".to_string())
                        })
                })
                .transpose()?;
            Ok(pb::Action {
                id: Uuid::now_v7().to_string(),
                journey_id: journey_id.to_string(),
                stage_id,
                title: template.title.trim().to_string(),
                detail: template.detail.trim().to_string(),
                estimated_minutes: template.estimated_minutes,
                scheduled_label: template.scheduled_label.trim().to_string(),
                scheduled_for: None,
                scheduled_timezone: None,
                recurrence: None,
                state: pb::ActionState::Pending as i32,
            })
        })
        .collect()
}

fn normalized_completion_criteria(value: String, journey_type: i32) -> String {
    let value = value.trim();
    if !value.is_empty() {
        return value.to_string();
    }
    match pb::JourneyType::try_from(journey_type).unwrap_or(pb::JourneyType::Project) {
        pb::JourneyType::Habit => "在自己的周期内达到期望频率".to_string(),
        pb::JourneyType::Project => "完成路线中的必要阶段和行动".to_string(),
        pb::JourneyType::Quantity => "达到为这条路线设定的累计目标".to_string(),
        pb::JourneyType::Travel => "完成行前、在途和归来后的关键经历".to_string(),
        pb::JourneyType::Challenge => "在限定周期内满足这条挑战的条件".to_string(),
    }
}

fn validate_action_stage(
    stage_id: Option<&str>,
    stages: &[pb::JourneyStage],
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
    recurrence: Option<pb::ActionRecurrence>,
    scheduled_for: Option<&str>,
    scheduled_timezone: Option<&str>,
) -> Result<Option<pb::ActionRecurrence>, GrowthError> {
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
    match pb::ActionRecurrenceFrequency::try_from(recurrence.frequency) {
        Ok(pb::ActionRecurrenceFrequency::Daily) if !recurrence.weekdays.is_empty() => {
            return Err(GrowthError::Validation(
                "按日重复不能设置星期几".to_string(),
            ));
        }
        Ok(pb::ActionRecurrenceFrequency::Weekly) if recurrence.weekdays.is_empty() => {
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

fn weekday_order(weekday: &i32) -> u8 {
    match pb::Weekday::try_from(*weekday).unwrap_or(pb::Weekday::Sunday) {
        pb::Weekday::Monday => 0,
        pb::Weekday::Tuesday => 1,
        pb::Weekday::Wednesday => 2,
        pb::Weekday::Thursday => 3,
        pb::Weekday::Friday => 4,
        pb::Weekday::Saturday => 5,
        pb::Weekday::Sunday => 6,
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

fn validate_notification(request: &pb::CreateNotificationRequest) -> Result<(), GrowthError> {
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

fn validate_knowledge_create(request: &pb::CreateKnowledgeRequest) -> Result<(), GrowthError> {
    validate_text(&request.title, 1, 200, "资源标题")?;
    validate_text(&request.creator, 0, 120, "作者或来源")?;
    validate_text(&request.summary, 0, 1_000, "摘要")?;
    validate_optional_source_url(request.source_url.as_deref())?;
    validate_optional_text(request.body.as_deref(), 500_000, "资源正文")?;
    validate_tags(&request.tags)?;
    validate_optional_text(request.journey_id.as_deref(), 128, "路线标识")?;
    validate_optional_text(request.source_content_id.as_deref(), 160, "社区内容标识")
}

fn validate_knowledge_update(request: &pb::UpdateKnowledgeRequest) -> Result<(), GrowthError> {
    if let Some(title) = request.title.as_deref() {
        validate_text(title, 1, 200, "资源标题")?;
    }
    if let Some(creator) = request.creator.as_deref() {
        validate_text(creator, 0, 120, "作者或来源")?;
    }
    if let Some(summary) = request.summary.as_deref() {
        validate_text(summary, 0, 1_000, "摘要")?;
    }
    validate_optional_source_url(request.source_url.as_deref())?;
    validate_optional_text(request.body.as_deref(), 500_000, "资源正文")?;
    validate_optional_text(request.journey_id.as_deref(), 128, "路线标识")?;
    if let Some(tags) = request.tags.as_ref() {
        validate_tags(&tags.values)?;
    }
    if request.progress.is_some_and(|progress| progress > 100) {
        return Err(GrowthError::Validation(
            "阅读进度需要在 0 到 100 之间".to_string(),
        ));
    }
    if let Some(bookmarks) = request.bookmarks.as_ref() {
        if bookmarks.values.len() > 500 {
            return Err(GrowthError::Validation("书签不能超过 500 个".to_string()));
        }
        for bookmark in &bookmarks.values {
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

fn validate_optional_source_url(value: Option<&str>) -> Result<(), GrowthError> {
    validate_optional_text(value, 2_048, "来源地址")?;
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let url = Url::parse(value)
        .map_err(|_| GrowthError::Validation("来源地址必须是有效的 URL".to_string()))?;
    let is_web_url = matches!(url.scheme(), "http" | "https") && url.host_str().is_some();
    let is_content_reference = url.scheme() == "bookway"
        && url.host_str() == Some("content")
        && url.query().is_none()
        && url.fragment().is_none()
        && url
            .path_segments()
            .is_some_and(|mut segments| matches!((segments.next(), segments.next()), (Some(id), None) if !id.is_empty() && id.chars().count() <= 160));
    if is_web_url || is_content_reference {
        Ok(())
    } else {
        Err(GrowthError::Validation(
            "来源地址仅支持 http(s) URL 或 bookway 内容引用".to_string(),
        ))
    }
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

fn validate_entry(request: &pb::CreateEntryRequest) -> Result<(), GrowthError> {
    if request.published && request.body.trim().is_empty() {
        return Err(GrowthError::Validation("发布行记需要填写正文".to_string()));
    }
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
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(GrowthError::Validation(
            "图片必须使用已上传的媒体资源 ID，不能直接提供 URL".to_string(),
        ));
    }
    if let Some(media_id) = request.photo_media_id.as_deref()
        && Uuid::parse_str(media_id.trim()).is_err()
    {
        return Err(GrowthError::Validation(
            "图片媒体资源 ID 格式不正确".to_string(),
        ));
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

fn domain_order(domain: i32) -> u8 {
    match pb::GrowthDomain::try_from(domain).unwrap_or(pb::GrowthDomain::Leisure) {
        pb::GrowthDomain::Learning => 0,
        pb::GrowthDomain::Movement => 1,
        pb::GrowthDomain::Wellness => 2,
        pb::GrowthDomain::Travel => 3,
        pb::GrowthDomain::Leisure => 4,
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
) -> Vec<pb::ReviewAdjustmentSuggestion> {
    let mut suggestions = Vec::new();
    let decided_actions = completed_actions + skipped_actions;
    if completion_rate < 0.5
        && (skipped_actions > 0 || decided_actions >= 3)
        && let Some(action) = snapshot
            .actions
            .iter()
            .filter(|action| action.state == pb::ActionState::Pending as i32)
            .max_by_key(|action| action.estimated_minutes)
    {
        let suggested_minutes = recovery_minutes(action.estimated_minutes);
        suggestions.push(pb::ReviewAdjustmentSuggestion {
            kind: pb::ReviewAdjustmentKind::ReduceActionDuration as i32,
            title: format!("把「{}」先缩小到 {suggested_minutes} 分钟", action.title),
            rationale: "这周出现了中断。缩小下一步不会抹掉原计划，只是为恢复节奏留出更低的门槛。"
                .to_string(),
            action_patch: Some(pb::ReviewActionPatch {
                action_id: action.id.clone(),
                estimated_minutes: Some(suggested_minutes),
                scheduled_label: None,
                expected_estimated_minutes: Some(action.estimated_minutes),
            }),
            journey_patch: None,
        });
    }

    for journey in snapshot
        .journeys
        .iter()
        .filter(|journey| journey.status == pb::JourneyStatus::Active as i32)
    {
        let actions = snapshot
            .actions
            .iter()
            .filter(|action| action.journey_id == journey.id)
            .collect::<Vec<_>>();
        if !actions.is_empty()
            && actions
                .iter()
                .all(|action| action.state == pb::ActionState::Skipped as i32)
        {
            suggestions.push(pb::ReviewAdjustmentSuggestion {
                kind: pb::ReviewAdjustmentKind::PauseJourney as i32,
                title: format!("先暂停「{}」", journey.title),
                rationale:
                    "这条路线的行动都被跳过了。暂停是保留计划与记录的选择，准备好后仍可继续。"
                        .to_string(),
                action_patch: None,
                journey_patch: Some(pb::ReviewJourneyPatch {
                    journey_id: journey.id.clone(),
                    status: pb::JourneyStatus::Paused as i32,
                    expected_status: Some(pb::JourneyStatus::Active as i32),
                }),
            });
        }
    }
    suggestions
}

fn recovery_minutes(estimated_minutes: u32) -> u32 {
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
    request: &pb::UpdateReminderPreferencesRequest,
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

fn action_is_overdue(action: &pb::Action) -> bool {
    action
        .scheduled_for
        .as_deref()
        .and_then(|timestamp| OffsetDateTime::parse(timestamp, &Rfc3339).ok())
        .is_some_and(|timestamp| timestamp < OffsetDateTime::now_utc())
}

fn validate_journey_update(request: &pb::UpdateJourneyRequest) -> Result<(), GrowthError> {
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
    estimated_minutes: u32,
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

fn count_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{conf::Config, datasource::MemoryGrowthDao};

    fn domain() -> Domain {
        Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            },
            Arc::new(MemoryGrowthDao::seeded()),
        )
    }

    async fn saved_review_with_reduce_suggestion(
        domain: &Domain,
        user_id: &str,
    ) -> (pb::ReviewRecord, pb::Action, u32) {
        let journey = domain
            .create_journey(
                user_id,
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "恢复写作节奏".to_string(),
                    intent: "把写作重新安排回一周".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
                },
            )
            .await
            .expect("journey should be created");
        let first = domain
            .get_journey(user_id, &journey.id)
            .await
            .expect("journey should load")
            .actions
            .into_iter()
            .next()
            .expect("first action should exist");
        domain
            .update_action(
                user_id,
                &first.id,
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
                    state: Some(pb::ActionState::Skipped as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("first action should be skippable");
        let follow_up = domain
            .create_action(
                user_id,
                pb::CreateActionRequest {
                    user_id: String::new(),
                    journey_id: journey.id,
                    stage_id: None,
                    title: "修改一段".to_string(),
                    detail: String::new(),
                    estimated_minutes: 30,
                    scheduled_label: "本周".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                    idempotency_key: None,
                },
            )
            .await
            .expect("follow-up action should be created");
        let review = domain
            .save_weekly_review(
                user_id,
                pb::SaveWeeklyReviewRequest {
                    user_id: String::new(),
                    reflection: "先降低开始门槛".to_string(),
                    next_focus: "完成一段短写".to_string(),
                },
            )
            .await
            .expect("review should be saved");
        let suggestion_index = review
            .summary
            .as_ref()
            .expect("review should keep its summary")
            .adjustment_suggestions
            .iter()
            .position(|suggestion| {
                suggestion.kind == pb::ReviewAdjustmentKind::ReduceActionDuration as i32
            })
            .and_then(|index| u32::try_from(index).ok())
            .expect("review should offer a smaller next step");
        (review, follow_up, suggestion_index)
    }

    #[test]
    fn accepts_openable_and_trusted_knowledge_source_urls_only() {
        for source in [
            "https://example.com/learn?topic=walk",
            "http://localhost:8080/resource",
            "bookway://content/post-reading",
        ] {
            assert!(
                validate_optional_source_url(Some(source)).is_ok(),
                "{source}"
            );
        }
        for source in [
            "javascript:alert(1)",
            "file:///private/resource",
            "https://",
            "bookway://journey/secret",
            "bookway://content/post-reading/extra",
        ] {
            assert!(
                validate_optional_source_url(Some(source)).is_err(),
                "{source}"
            );
        }
    }

    #[tokio::test]
    async fn creates_a_journey_with_its_first_action() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "user-a",
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "学习摄影".to_string(),
                    intent: "记录旅行".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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
    async fn journey_creation_retries_return_the_original_journey_after_later_edits() {
        let domain = domain();
        let request = pb::CreateJourneyRequest {
            user_id: String::new(),
            title: "晨间阅读计划".to_string(),
            intent: "每天保留一段稳定阅读时间".to_string(),
            domain: pb::GrowthDomain::Learning as i32,
            journey_type: pb::JourneyType::Habit as i32,
            completion_criteria: "连续完成七次阅读".to_string(),
            stages: vec![pb::JourneyStageInput {
                title: "先建立节奏".to_string(),
                detail: "从一本正在读的书开始".to_string(),
                completion_criteria: "完成前三次阅读".to_string(),
            }],
            duration_label: "两周".to_string(),
            first_action_title: "阅读十分钟".to_string(),
            first_action_detail: "只读一个段落也算开始".to_string(),
            estimated_minutes: 10,
            first_action_scheduled_label: Some("今晚".to_string()),
            first_action_scheduled_for: None,
            first_action_scheduled_timezone: None,
            first_action_stage_index: Some(0),
            first_action_recurrence: None,
            idempotency_key: Some("journey-create-1".to_string()),
        };

        let first = domain
            .create_journey("journey-idempotency-user", request.clone())
            .await
            .expect("first Journey should persist");
        let first_action = domain
            .get_journey("journey-idempotency-user", &first.id)
            .await
            .expect("Journey should load")
            .actions
            .into_iter()
            .next()
            .expect("Journey should have its initial action");
        domain
            .update_journey(
                "journey-idempotency-user",
                &first.id,
                pb::UpdateJourneyRequest {
                    user_id: String::new(),
                    journey_id: String::new(),
                    title: Some("后来调整过的阅读计划".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("Journey should update");
        domain
            .update_action(
                "journey-idempotency-user",
                &first_action.id,
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
                    title: Some("阅读五分钟".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("initial action should update");

        let retry = domain
            .create_journey("journey-idempotency-user", request.clone())
            .await
            .expect("matching retry should return the original Journey");
        assert_eq!(retry.id, first.id);
        assert_eq!(retry.title, "后来调整过的阅读计划");
        assert_eq!(
            domain
                .list_journeys("journey-idempotency-user")
                .await
                .expect("Journeys should load")
                .len(),
            1
        );

        let mut conflicting = request;
        conflicting.first_action_title = "改成另一项首要行动".to_string();
        let error = domain
            .create_journey("journey-idempotency-user", conflicting)
            .await
            .expect_err("one idempotency key cannot describe another Journey");
        assert!(matches!(
            error,
            GrowthError::Dao(crate::datasource::DaoError::IdempotencyConflict)
        ));
    }

    #[tokio::test]
    async fn action_creation_retries_return_the_original_action_without_duplication() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "action-idempotency-user",
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "稳定阅读计划".to_string(),
                    intent: "每天留下可回看的阅读时间".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Habit as i32,
                    completion_criteria: "连续完成七次阅读".to_string(),
                    stages: Vec::new(),
                    duration_label: "2 周".to_string(),
                    first_action_title: "阅读十分钟".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 10,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                    idempotency_key: None,
                },
            )
            .await
            .expect("journey should be created");
        let request = pb::CreateActionRequest {
            user_id: String::new(),
            journey_id: journey.id.clone(),
            stage_id: None,
            title: "记录一个观点".to_string(),
            detail: "写下刚读到的一个想法".to_string(),
            estimated_minutes: 12,
            scheduled_label: "今晚".to_string(),
            scheduled_for: None,
            scheduled_timezone: None,
            recurrence: None,
            idempotency_key: Some("action-create-1".to_string()),
        };

        let first = domain
            .create_action("action-idempotency-user", request.clone())
            .await
            .expect("first action should be created");
        let replay = domain
            .create_action("action-idempotency-user", request.clone())
            .await
            .expect("retry should return the existing action");
        assert_eq!(replay.id, first.id);
        assert_eq!(
            domain
                .get_journey("action-idempotency-user", &journey.id)
                .await
                .expect("journey should load")
                .actions
                .len(),
            2
        );

        let conflict = domain
            .create_action(
                "action-idempotency-user",
                pb::CreateActionRequest {
                    title: "换成另一条行动".to_string(),
                    ..request
                },
            )
            .await;
        assert!(matches!(
            conflict,
            Err(GrowthError::Dao(
                crate::datasource::DaoError::IdempotencyConflict
            ))
        ));
    }

    #[tokio::test]
    async fn connects_actions_to_stages_and_materializes_the_next_repeat() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "repeat-user",
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "晨间跑步恢复计划".to_string(),
                    intent: "用低压力节奏重新开始跑步".to_string(),
                    domain: pb::GrowthDomain::Movement as i32,
                    journey_type: pb::JourneyType::Habit as i32,
                    completion_criteria: "每周完成三次轻松跑".to_string(),
                    stages: vec![
                        pb::JourneyStageInput {
                            title: "恢复节奏".to_string(),
                            detail: "先保持轻松和可恢复".to_string(),
                            completion_criteria: "完成三次轻松跑".to_string(),
                        },
                        pb::JourneyStageInput {
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
                    first_action_recurrence: Some(pb::ActionRecurrence {
                        frequency: pb::ActionRecurrenceFrequency::Weekly as i32,
                        interval: 1,
                        weekdays: vec![pb::Weekday::Monday as i32, pb::Weekday::Thursday as i32],
                        ends_on: Some("2032-06-30".to_string()),
                        anchor_date: None,
                    }),
                    idempotency_key: None,
                },
            )
            .await
            .expect("journey should be created");
        assert_eq!(journey.journey_type, pb::JourneyType::Habit as i32);
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
        assert_eq!(successor.state, pb::ActionState::Pending as i32);
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
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "重新开始写作".to_string(),
                    intent: "让写作回到一周安排里".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
                    state: Some(pb::ActionState::Skipped as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("action should be skippable");
        let follow_up = domain
            .create_action(
                "review-adjust-user",
                pb::CreateActionRequest {
                    user_id: String::new(),
                    journey_id: journey.id,
                    stage_id: None,
                    title: "修改一段".to_string(),
                    detail: String::new(),
                    estimated_minutes: 30,
                    scheduled_label: "本周".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                    idempotency_key: None,
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
            .find(|suggestion| {
                suggestion.kind == pb::ReviewAdjustmentKind::ReduceActionDuration as i32
            })
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
    async fn saves_one_weekly_review_without_rewriting_its_snapshot() {
        let domain = domain();
        let first = domain
            .save_weekly_review(
                "demo-user",
                pb::SaveWeeklyReviewRequest {
                    user_id: String::new(),
                    reflection: "午间阅读比晚间更容易坚持".to_string(),
                    next_focus: "先完成每天 20 分钟阅读".to_string(),
                },
            )
            .await
            .expect("first review should persist");
        let first_summary = first.summary.clone().expect("summary should be retained");

        let updated = domain
            .save_weekly_review(
                "demo-user",
                pb::SaveWeeklyReviewRequest {
                    user_id: String::new(),
                    reflection: "午间阅读和散步一起安排更可持续".to_string(),
                    next_focus: "先完成每天 20 分钟阅读和一次散步".to_string(),
                },
            )
            .await
            .expect("review edits should persist");

        assert_eq!(updated.id, first.id);
        assert_eq!(updated.created_at, first.created_at);
        assert_eq!(updated.summary, Some(first_summary));
        assert_eq!(updated.reflection, "午间阅读和散步一起安排更可持续");
        assert_eq!(updated.next_focus, "先完成每天 20 分钟阅读和一次散步");
    }

    #[tokio::test]
    async fn applies_a_saved_review_suggestion_once_and_records_its_decision() {
        let domain = domain();
        let (review, follow_up, suggestion_index) =
            saved_review_with_reduce_suggestion(&domain, "review-apply-user").await;
        let applied = domain
            .apply_weekly_review_adjustment(
                "review-apply-user",
                pb::ApplyWeeklyReviewAdjustmentRequest {
                    user_id: String::new(),
                    review_id: review.id.clone(),
                    suggestion_index,
                },
            )
            .await
            .expect("saved suggestion should apply");
        let decision = applied.decision.expect("decision should be returned");
        assert_eq!(decision.suggestion_index, suggestion_index);
        assert_eq!(
            decision
                .action
                .as_ref()
                .map(|action| action.estimated_minutes),
            Some(10)
        );
        assert_eq!(
            applied
                .review
                .as_ref()
                .map(|review| review.applied_adjustments.len()),
            Some(1)
        );
        let action = domain
            .get_journey("review-apply-user", &follow_up.journey_id)
            .await
            .expect("journey should load")
            .actions
            .into_iter()
            .find(|action| action.id == follow_up.id)
            .expect("follow-up should remain in the journey");
        assert_eq!(action.estimated_minutes, 10);

        let retry = domain
            .apply_weekly_review_adjustment(
                "review-apply-user",
                pb::ApplyWeeklyReviewAdjustmentRequest {
                    user_id: String::new(),
                    review_id: review.id.clone(),
                    suggestion_index,
                },
            )
            .await
            .expect("retry should return the original decision");
        assert_eq!(retry.decision, Some(decision));
        assert!(matches!(
            domain
                .apply_weekly_review_adjustment(
                    "another-user",
                    pb::ApplyWeeklyReviewAdjustmentRequest {
                        user_id: String::new(),
                        review_id: review.id,
                        suggestion_index,
                    },
                )
                .await,
            Err(GrowthError::Dao(
                crate::datasource::DaoError::ReviewNotFound(_)
            ))
        ));
    }

    #[tokio::test]
    async fn rejects_a_review_suggestion_after_its_target_changes() {
        let domain = domain();
        let (review, follow_up, suggestion_index) =
            saved_review_with_reduce_suggestion(&domain, "review-stale-user").await;
        domain
            .update_action(
                "review-stale-user",
                &follow_up.id,
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
                    estimated_minutes: Some(20),
                    ..Default::default()
                },
            )
            .await
            .expect("manual action edit should succeed");

        assert!(matches!(
            domain
                .apply_weekly_review_adjustment(
                    "review-stale-user",
                    pb::ApplyWeeklyReviewAdjustmentRequest {
                        user_id: String::new(),
                        review_id: review.id,
                        suggestion_index,
                    },
                )
                .await,
            Err(GrowthError::Dao(
                crate::datasource::DaoError::ReviewAdjustmentStale
            ))
        ));
    }

    #[tokio::test]
    async fn route_journey_retries_reuse_the_same_private_journey() {
        let domain = domain();
        let request = pb::CreateJourneyRequest {
            user_id: String::new(),
            title: "四周写作练习".to_string(),
            intent: "建立稳定节奏".to_string(),
            domain: pb::GrowthDomain::Learning as i32,
            journey_type: pb::JourneyType::Project as i32,
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
            idempotency_key: None,
        };
        let (first, retry) = tokio::join!(
            domain.create_route_journey("user-a", "route-a", request.clone(), Vec::new()),
            domain.create_route_journey("user-a", "route-a", request.clone(), Vec::new()),
        );
        let first = first.expect("first route join");
        let retry = retry.expect("concurrent route join retry");
        let other_user = domain
            .create_route_journey("user-b", "route-a", request, Vec::new())
            .await
            .expect("other user route join");
        let after_source_edit = domain
            .create_route_journey(
                "user-a",
                "route-a",
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "来源内容更新后的标题".to_string(),
                    intent: String::new(),
                    domain: pb::GrowthDomain::Leisure as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
                },
                Vec::new(),
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
    async fn route_templates_copy_stages_and_actions_into_private_journeys() {
        let domain = domain();
        let journey = domain
            .create_route_journey(
                "template-user",
                "route-template",
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "四周主题阅读".to_string(),
                    intent: "从问题出发建立阅读方法".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
                    completion_criteria: "完成四次阅读和一次回望".to_string(),
                    stages: vec![
                        pb::JourneyStageInput {
                            title: "选题".to_string(),
                            detail: "选择一个真实问题".to_string(),
                            completion_criteria: "确定阅读问题".to_string(),
                        },
                        pb::JourneyStageInput {
                            title: "沉淀".to_string(),
                            detail: "把结论写成自己的话".to_string(),
                            completion_criteria: "完成一篇回望".to_string(),
                        },
                    ],
                    duration_label: "4 周".to_string(),
                    first_action_title: "选一本起步书".to_string(),
                    first_action_detail: "只选一本最相关的书".to_string(),
                    estimated_minutes: 15,
                    first_action_scheduled_label: Some("今天".to_string()),
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: Some(0),
                    first_action_recurrence: None,
                    idempotency_key: None,
                },
                vec![
                    pb::RouteActionTemplate {
                        title: "读二十分钟".to_string(),
                        detail: "标记一个值得验证的观点".to_string(),
                        estimated_minutes: 20,
                        scheduled_label: "本周".to_string(),
                        stage_index: Some(0),
                    },
                    pb::RouteActionTemplate {
                        title: "写一段回望".to_string(),
                        detail: "记录哪些方法适合自己".to_string(),
                        estimated_minutes: 25,
                        scheduled_label: "第四周".to_string(),
                        stage_index: Some(1),
                    },
                ],
            )
            .await
            .expect("route template should create a private journey");

        let detail = domain
            .get_journey("template-user", &journey.id)
            .await
            .expect("private journey should load");
        assert_eq!(detail.journey.expect("journey").stages.len(), 2);
        assert_eq!(detail.actions.len(), 3);
        assert!(
            detail
                .actions
                .iter()
                .all(|action| action.stage_id.is_some())
        );
        let action = detail
            .actions
            .iter()
            .find(|action| action.title == "读二十分钟")
            .expect("template action should exist");
        let completion = domain
            .complete_action("template-user", &action.id)
            .await
            .expect("adopted action should complete");
        assert_eq!(
            completion.source_route_id.as_deref(),
            Some("route-template")
        );
        assert_eq!(
            completion.action.map(|action| action.state),
            Some(pb::ActionState::Completed as i32)
        );
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
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: " ".to_string(),
                    intent: String::new(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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
                pb::UpdateReminderPreferencesRequest {
                    user_id: String::new(),
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
                pb::UpdateReminderPreferencesRequest {
                    user_id: String::new(),
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
                pb::UpdateReminderPreferencesRequest {
                    user_id: String::new(),
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
                pb::RegisterPushDeviceRequest {
                    user_id: String::new(),
                    device_id: "ios-installation-1".to_string(),
                    provider: pb::PushProvider::Expo as i32,
                    endpoint: "ExponentPushToken[opaque]".to_string(),
                },
            )
            .await
            .expect("device should register");

        assert_eq!(device.device_id, "ios-installation-1");
        assert_eq!(device.provider, pb::PushProvider::Expo as i32);
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
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "四周写作练习".to_string(),
                    intent: "留下可回看的作品".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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
                pb::CreateActionRequest {
                    user_id: String::new(),
                    journey_id: journey.id.clone(),
                    stage_id: None,
                    title: "修改开头".to_string(),
                    detail: "让第一段更具体".to_string(),
                    estimated_minutes: 15,
                    scheduled_label: "明天".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                    idempotency_key: None,
                },
            )
            .await
            .expect("action should be created");
        let updated = domain
            .update_action(
                "user-a",
                &action.id,
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
                    state: Some(pb::ActionState::Skipped as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("action should update");
        assert_eq!(updated.state, pb::ActionState::Skipped as i32);

        let paused = domain
            .update_journey(
                "user-a",
                &journey.id,
                pb::UpdateJourneyRequest {
                    user_id: String::new(),
                    journey_id: String::new(),
                    status: Some(pb::JourneyStatus::Paused as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("journey should update");
        assert_eq!(paused.status, pb::JourneyStatus::Paused as i32);
    }

    #[tokio::test]
    async fn persists_entries_and_builds_a_review_from_real_actions() {
        let domain = domain();
        let entry = domain
            .create_entry(
                "demo-user",
                pb::CreateEntryRequest {
                    user_id: String::new(),
                    action_id: Some("action-stretch".to_string()),
                    journey_id: Some("journey-running".to_string()),
                    body: "跑后身体很放松".to_string(),
                    mood: pb::EntryMood::Calm as i32,
                    duration_minutes: Some(8),
                    quantity: None,
                    location: None,
                    photo_url: None,
                    published: false,
                    photo_media_id: None,
                    idempotency_key: None,
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
    async fn entry_creation_retries_return_the_original_entry_without_duplication() {
        let domain = domain();
        let request = pb::CreateEntryRequest {
            user_id: String::new(),
            action_id: Some("action-stretch".to_string()),
            journey_id: Some("journey-running".to_string()),
            body: "跑后留下一句身体感受".to_string(),
            mood: pb::EntryMood::Calm as i32,
            duration_minutes: Some(8),
            quantity: None,
            location: None,
            photo_url: None,
            published: true,
            photo_media_id: None,
            idempotency_key: Some("entry-create-1".to_string()),
        };

        let first = domain
            .create_entry("demo-user", request.clone())
            .await
            .expect("first entry should persist");
        let retry = domain
            .create_entry("demo-user", request.clone())
            .await
            .expect("matching retry should return the original entry");

        assert_eq!(retry.id, first.id);
        assert_eq!(
            domain
                .list_entries("demo-user")
                .await
                .expect("entries should load")
                .len(),
            1
        );

        let mut conflicting = request;
        conflicting.body = "不同的复盘内容".to_string();
        let error = domain
            .create_entry("demo-user", conflicting)
            .await
            .expect_err("a reused key cannot represent another entry");
        assert!(matches!(
            error,
            GrowthError::Dao(crate::datasource::DaoError::IdempotencyConflict)
        ));
    }

    #[tokio::test]
    async fn public_entry_request_starts_as_a_durable_pending_publication() {
        let domain = domain();
        let entry = domain
            .create_entry(
                "demo-user",
                pb::CreateEntryRequest {
                    user_id: String::new(),
                    action_id: None,
                    journey_id: Some("journey-reading".to_string()),
                    body: "今天把一个困惑写清楚了。".to_string(),
                    mood: pb::EntryMood::Clear as i32,
                    duration_minutes: Some(20),
                    quantity: Some("1 个问题".to_string()),
                    location: Some("private location".to_string()),
                    photo_url: None,
                    published: true,
                    photo_media_id: None,
                    idempotency_key: None,
                },
            )
            .await
            .expect("entry should be accepted");

        assert!(!entry.published);
        assert_eq!(
            entry.publication_status,
            pb::EntryPublicationStatus::Pending as i32
        );
        assert!(entry.public_content_id.is_none());
        assert!(entry.publication_error.is_none());
    }

    #[tokio::test]
    async fn companion_offers_a_small_recovery_step_without_changing_the_plan() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "recovering-user",
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "恢复阅读节奏".to_string(),
                    intent: "重新建立低压力阅读习惯".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
                    state: Some(pb::ActionState::Skipped as i32),
                    ..Default::default()
                },
            )
            .await
            .expect("action should be skippable");
        let pending = domain
            .create_action(
                "recovering-user",
                pb::CreateActionRequest {
                    user_id: String::new(),
                    journey_id: journey.id,
                    stage_id: None,
                    title: "读两页".to_string(),
                    detail: String::new(),
                    estimated_minutes: 24,
                    scheduled_label: "今晚".to_string(),
                    scheduled_for: None,
                    scheduled_timezone: None,
                    recurrence: None,
                    idempotency_key: None,
                },
            )
            .await
            .expect("next action should be created");

        let brief = domain
            .companion_brief_for("recovering-user", None, None)
            .await
            .expect("companion brief should load");

        assert_eq!(brief.mode, pb::CompanionMode::StartSmall as i32);
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
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "傍晚散步".to_string(),
                    intent: "把散步留给下班后的自己".to_string(),
                    domain: pb::GrowthDomain::Wellness as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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
                pb::UpdateActionRequest {
                    user_id: String::new(),
                    action_id: String::new(),
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
                pb::CreateActionRequest {
                    user_id: String::new(),
                    journey_id: journey.id,
                    stage_id: None,
                    title: "不完整安排".to_string(),
                    detail: String::new(),
                    estimated_minutes: 10,
                    scheduled_label: "稍后".to_string(),
                    scheduled_for: Some("2032-06-18T20:00:00+08:00".to_string()),
                    scheduled_timezone: None,
                    recurrence: None,
                    idempotency_key: None,
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
                pb::CreateJourneyRequest {
                    user_id: String::new(),
                    title: "重新建立写作节奏".to_string(),
                    intent: "把写作放回每周安排".to_string(),
                    domain: pb::GrowthDomain::Learning as i32,
                    journey_type: pb::JourneyType::Project as i32,
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
                    idempotency_key: None,
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

        assert_eq!(brief.mode, pb::CompanionMode::StartSmall as i32);
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

        assert_eq!(brief.mode, pb::CompanionMode::PlanNext as i32);
        assert!(brief.suggested_action.is_none());
        assert_eq!(brief.active_journeys, 0);
    }

    #[tokio::test]
    async fn persists_searches_and_updates_private_knowledge_resources() {
        let domain = domain();
        let request = pb::CreateKnowledgeRequest {
            user_id: String::new(),
            idempotency_key: None,
            title: " 看不见的城市 ".to_string(),
            creator: "伊塔洛·卡尔维诺".to_string(),
            summary: "关于城市、记忆与欲望".to_string(),
            kind: pb::KnowledgeResourceKind::Book as i32,
            status: pb::KnowledgeResourceStatus::Active as i32,
            source_url: None,
            body: Some("第一章\n城市与记忆".to_string()),
            tags: vec!["城市".to_string(), " 文学 ".to_string()],
            journey_id: Some("journey-reading".to_string()),
            source_content_id: None,
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
                .list_knowledge("another-user", pb::KnowledgeQueryRequest::default(),)
                .await
                .expect("another user's resources should load")
                .is_empty()
        );
        let matches = domain
            .list_knowledge(
                "demo-user",
                pb::KnowledgeQueryRequest {
                    q: Some("记忆".to_string()),
                    kind: Some(pb::KnowledgeResourceKind::Book as i32),
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
                pb::UpdateKnowledgeRequest {
                    progress: Some(100),
                    current_position: Some(2),
                    reading_seconds: Some(900),
                    bookmarks: Some(pb::StringList {
                        values: vec!["城市与记忆".to_string()],
                    }),
                    last_opened_at: Some("2026-08-15T08:00:00Z".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("knowledge resource should update");
        assert_eq!(updated.progress, 100);
        assert_eq!(
            updated.status,
            pb::KnowledgeResourceStatus::Completed as i32
        );
        assert_eq!(updated.bookmarks, vec!["城市与记忆"]);
    }

    #[tokio::test]
    async fn rejects_knowledge_idempotency_conflicts_and_foreign_routes() {
        let domain = domain();
        let request = pb::CreateKnowledgeRequest {
            user_id: String::new(),
            idempotency_key: None,
            title: "第一本书".to_string(),
            creator: String::new(),
            summary: String::new(),
            kind: pb::KnowledgeResourceKind::Book as i32,
            status: pb::KnowledgeResourceStatus::Inbox as i32,
            source_url: None,
            body: None,
            tags: Vec::new(),
            journey_id: None,
            source_content_id: None,
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
            Err(GrowthError::Dao(
                crate::datasource::DaoError::IdempotencyConflict
            ))
        ));
        let foreign_route = pb::CreateKnowledgeRequest {
            user_id: String::new(),
            idempotency_key: None,
            title: "越权资源".to_string(),
            creator: String::new(),
            summary: String::new(),
            kind: pb::KnowledgeResourceKind::Note as i32,
            status: pb::KnowledgeResourceStatus::Inbox as i32,
            source_url: None,
            body: None,
            tags: Vec::new(),
            journey_id: Some("journey-reading".to_string()),
            source_content_id: None,
        };
        assert!(
            domain
                .create_knowledge("another-user", foreign_route, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn deduplicates_community_knowledge_references_across_content_edits() {
        let domain = domain();
        let request = pb::CreateKnowledgeRequest {
            user_id: String::new(),
            idempotency_key: Some("knowledge-capture:post-reading".to_string()),
            title: "旧标题".to_string(),
            creator: "作者".to_string(),
            summary: "第一版摘要".to_string(),
            kind: pb::KnowledgeResourceKind::Article as i32,
            status: pb::KnowledgeResourceStatus::Inbox as i32,
            source_url: Some("bookway://content/post-reading".to_string()),
            body: None,
            tags: vec!["阅读".to_string()],
            journey_id: None,
            source_content_id: Some("post-reading".to_string()),
        };
        let captured = domain
            .create_knowledge(
                "demo-user",
                request.clone(),
                request.idempotency_key.clone(),
            )
            .await
            .expect("first public reference should be captured");
        let mut edited = request;
        edited.title = "新标题".to_string();
        edited.summary = "第二版摘要".to_string();
        let retried = domain
            .create_knowledge("demo-user", edited.clone(), edited.idempotency_key.clone())
            .await
            .expect("an edit must return the existing private reference");

        assert_eq!(retried.id, captured.id);
        assert_eq!(retried.title, "旧标题");
        assert_eq!(retried.source_content_id.as_deref(), Some("post-reading"));
    }

    #[tokio::test]
    async fn turns_one_private_resource_into_one_private_journey() {
        let domain = domain();
        let resource = domain
            .create_knowledge(
                "demo-user",
                pb::CreateKnowledgeRequest {
                    user_id: String::new(),
                    idempotency_key: Some("knowledge-journey-source".to_string()),
                    title: "街区观察方法".to_string(),
                    creator: "作者".to_string(),
                    summary: "从一次步行开始建立观察练习。".to_string(),
                    kind: pb::KnowledgeResourceKind::Article as i32,
                    status: pb::KnowledgeResourceStatus::Inbox as i32,
                    source_url: Some("bookway://content/city-walk".to_string()),
                    body: None,
                    tags: vec!["城市".to_string(), "观察".to_string()],
                    journey_id: None,
                    source_content_id: Some("city-walk".to_string()),
                },
                Some("knowledge-journey-source".to_string()),
            )
            .await
            .expect("knowledge resource should persist");
        let request = pb::StartKnowledgeJourneyRequest {
            user_id: String::new(),
            resource_id: resource.id.clone(),
            title: "城市观察练习".to_string(),
            intent: "把阅读中的方法带到一次真实步行里".to_string(),
            domain: pb::GrowthDomain::Learning as i32,
            journey_type: pb::JourneyType::Project as i32,
            completion_criteria: "完成三次街区观察记录".to_string(),
            stages: Vec::new(),
            duration_label: "3 周".to_string(),
            first_action_title: "完成第一次 15 分钟街区观察".to_string(),
            first_action_detail: "带着一个问题散步，并记录三个细节。".to_string(),
            estimated_minutes: 15,
            first_action_scheduled_label: Some("今天".to_string()),
            first_action_scheduled_for: None,
            first_action_scheduled_timezone: None,
            first_action_stage_index: None,
            first_action_recurrence: None,
        };
        let (first, retry) = tokio::join!(
            domain.start_knowledge_journey("demo-user", &resource.id, request.clone()),
            domain.start_knowledge_journey("demo-user", &resource.id, request),
        );
        let first = first.expect("first conversion should work");
        let retry = retry.expect("concurrent retry should reuse the journey");
        let journey_id = first
            .resource
            .as_ref()
            .and_then(|resource| resource.journey_id.as_deref())
            .expect("converted resource should link its journey")
            .to_string();

        assert_eq!(
            retry
                .resource
                .as_ref()
                .and_then(|item| item.journey_id.as_deref()),
            Some(journey_id.as_str())
        );
        assert_eq!(
            first.resource.as_ref().map(|item| item.status),
            Some(pb::KnowledgeResourceStatus::Active as i32)
        );
        assert_eq!(
            first
                .journey
                .as_ref()
                .and_then(|detail| detail.journey.as_ref())
                .map(|journey| journey.id.as_str()),
            Some(journey_id.as_str())
        );
        let journey = domain
            .get_journey("demo-user", &journey_id)
            .await
            .expect("linked journey should be readable");
        assert_eq!(journey.actions.len(), 1);
        assert_eq!(journey.actions[0].title, "完成第一次 15 分钟街区观察");
        let completion = domain
            .complete_action("demo-user", &journey.actions[0].id)
            .await
            .expect("knowledge-derived action should complete");
        assert_eq!(
            completion.source_knowledge_content_id.as_deref(),
            Some("city-walk")
        );
        assert!(
            domain
                .start_knowledge_journey(
                    "another-user",
                    &resource.id,
                    pb::StartKnowledgeJourneyRequest {
                        user_id: String::new(),
                        resource_id: resource.id.clone(),
                        title: "不应越权创建".to_string(),
                        intent: String::new(),
                        domain: pb::GrowthDomain::Learning as i32,
                        journey_type: pb::JourneyType::Project as i32,
                        completion_criteria: String::new(),
                        stages: Vec::new(),
                        duration_label: String::new(),
                        first_action_title: "无效".to_string(),
                        first_action_detail: String::new(),
                        estimated_minutes: 10,
                        first_action_scheduled_label: None,
                        first_action_scheduled_for: None,
                        first_action_scheduled_timezone: None,
                        first_action_stage_index: None,
                        first_action_recurrence: None,
                    }
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn exposes_a_private_notification_inbox_and_marks_items_read_idempotently() {
        let domain = domain();
        let inbox = domain
            .list_notifications("demo-user", pb::NotificationQueryRequest::default())
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
                .list_notifications("demo-user", pb::NotificationQueryRequest::default())
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
        let request = pb::CreateNotificationRequest {
            user_id: String::new(),
            kind: pb::NotificationKind::Community as i32,
            source_id: "like:reader:post-city".to_string(),
            title: "收到一个赞".to_string(),
            body: "有人赞了你的内容".to_string(),
            data: [
                ("post_id".to_string(), "post-city".to_string()),
                ("actor_id".to_string(), "reader".to_string()),
            ]
            .into_iter()
            .collect(),
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
                .list_notifications("author", pb::NotificationQueryRequest::default())
                .await
                .expect("recipient inbox should load")
                .items
                .len(),
            1
        );
        assert!(
            domain
                .list_notifications("reader", pb::NotificationQueryRequest::default())
                .await
                .expect("other inbox should load")
                .items
                .is_empty()
        );
        assert!(matches!(
            domain.create_notification("reader", request).await,
            Err(GrowthError::Dao(
                crate::datasource::DaoError::NotificationSourceConflict(_)
            ))
        ));
    }
}
