use std::{collections::BTreeSet, sync::Arc};

use crate::{conf::Config, datasource::GrpcDataSource};

use super::{
    api::{
        ActionDto, CommentDto, CommentPageDto, CommentQueryRequest, CompanionBriefDto,
        ContentAppealDto, ContentAppealPageDto, ContentAppealQueryRequest, ContentDto,
        ContentPageDto, ContentQueryRequest, ContentReportActionDto, ContentReportDto,
        ContentReportPageDto, ContentReportQueryRequest, CreateActionRequest, CreateCommentRequest,
        CreateCommentResult, CreateContentAppealRequest, CreateContentReportRequest,
        CreateContentRequest, CreateGrowthEntryRequest, CreateJourneyRequest,
        CreateKnowledgeResourceRequest, CreateUserNotificationRequest, FeedDto, FeedQueryRequest,
        FollowRequest, GrowthEntryDto, JourneyDetailDto, JourneyDto, KnowledgeQueryRequest,
        KnowledgeResourceDto, MediaDto, MediaUploadRequest, MediaUploadResponse,
        NotificationKindDto, NotificationPageDto, NotificationQueryRequest, PushDeviceDto,
        ReactionDto, ReactionRequest, RegisterPushDeviceRequest, ReminderPreferencesDto,
        ReviewContentAppealRequest, ReviewContentReportRequest, RouteJoinResultDto,
        RouteParticipationDto, RouteParticipationStateDto, SearchQueryRequest, SearchResponseDto,
        SetRouteParticipationRequest, SocialContextDto, SocialEdgeTypeDto, SocialVisibilityDto,
        SuggestionQueryRequest, SuggestionResponseDto, TodayDto, UpdateActionRequest,
        UpdateContentRequest, UpdateJourneyRequest, UpdateKnowledgeResourceRequest,
        UpdateReminderPreferencesRequest, UserEventBatchRequest, UserEventIngestResponse,
        UserNotificationDto, WeeklyReviewDto,
    },
    datasource::{
        BbsDataSource, BbsFeedDataSource, BbsLinkDataSource, CommentDataSource,
        ContentAuditDataSource, LikeStatusDataSource, MediaDataSource, SearchMainDataSource,
        UpstreamError, UserEventDataSource,
    },
};

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) growth: Arc<GrpcDataSource>,
    pub(crate) bbs_feed: Arc<dyn BbsFeedDataSource>,
    pub(crate) bbs_link: Arc<dyn BbsLinkDataSource>,
    pub(crate) search_main: Arc<dyn SearchMainDataSource>,
    pub(crate) bbs: Arc<dyn BbsDataSource>,
    pub(crate) comment: Arc<dyn CommentDataSource>,
    pub(crate) like_status: Arc<dyn LikeStatusDataSource>,
    pub(crate) user_event: Arc<dyn UserEventDataSource>,
    pub(crate) media: Arc<dyn MediaDataSource>,
    pub(crate) content_audit: Arc<dyn ContentAuditDataSource>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let growth = Arc::new(GrpcDataSource::connect("growth", config.growth_url.clone()).await?);
        let bbs_feed =
            Arc::new(GrpcDataSource::connect("bbs-feed", config.bbs_feed_url.clone()).await?);
        let bbs_link =
            Arc::new(GrpcDataSource::connect("bbs-link", config.bbs_link_url.clone()).await?);
        let search_main =
            Arc::new(GrpcDataSource::connect("search-main", config.search_main_url.clone()).await?);
        let bbs = Arc::new(GrpcDataSource::connect("bbs", config.bbs_url.clone()).await?);
        let comment =
            Arc::new(GrpcDataSource::connect("comment", config.comment_url.clone()).await?);
        let like_status = Arc::new(
            GrpcDataSource::connect("commonlikestatus", config.like_status_url.clone()).await?,
        );
        let user_event =
            Arc::new(GrpcDataSource::connect("user-event", config.user_event_url.clone()).await?);
        let media = Arc::new(GrpcDataSource::connect("media", config.media_url.clone()).await?);
        let content_audit = Arc::new(
            GrpcDataSource::connect("content-audit", config.content_audit_url.clone()).await?,
        );
        Ok(Self {
            config,
            growth,
            bbs_feed,
            bbs_link,
            search_main,
            bbs,
            comment,
            like_status,
            user_event,
            media,
            content_audit,
        })
    }
}

impl Domain {
    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<JourneyDto>, UpstreamError> {
        self.growth.list_journeys(user_id).await
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        self.growth.create_journey(user_id, request).await
    }

    pub(crate) async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, UpstreamError> {
        self.growth.get_journey(user_id, journey_id).await
    }

    pub(crate) async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        self.growth
            .update_journey(user_id, journey_id, request)
            .await
    }

    pub(crate) async fn create_action(
        &self,
        user_id: &str,
        request: CreateActionRequest,
    ) -> Result<ActionDto, UpstreamError> {
        self.growth.create_action(user_id, request).await
    }

    pub(crate) async fn today(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<TodayDto, UpstreamError> {
        self.growth.today(user_id, local_date, timezone).await
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, UpstreamError> {
        self.growth.complete_action(user_id, action_id).await
    }

    pub(crate) async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, UpstreamError> {
        self.growth.update_action(user_id, action_id, request).await
    }

    pub(crate) async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<ReminderPreferencesDto, UpstreamError> {
        self.growth.reminder_preferences(user_id).await
    }

    pub(crate) async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: UpdateReminderPreferencesRequest,
    ) -> Result<ReminderPreferencesDto, UpstreamError> {
        self.growth
            .update_reminder_preferences(user_id, request)
            .await
    }

    pub(crate) async fn register_push_device(
        &self,
        user_id: &str,
        request: RegisterPushDeviceRequest,
    ) -> Result<PushDeviceDto, UpstreamError> {
        self.growth.register_push_device(user_id, request).await
    }

    pub(crate) async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), UpstreamError> {
        self.growth.revoke_push_device(user_id, device_id).await
    }

    pub(crate) async fn list_notifications(
        &self,
        user_id: &str,
        request: NotificationQueryRequest,
    ) -> Result<NotificationPageDto, UpstreamError> {
        self.growth.list_notifications(user_id, request).await
    }

    pub(crate) async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<UserNotificationDto, UpstreamError> {
        self.growth
            .mark_notification_read(user_id, notification_id)
            .await
    }

    pub(crate) async fn list_entries(
        &self,
        user_id: &str,
    ) -> Result<Vec<GrowthEntryDto>, UpstreamError> {
        self.growth.list_entries(user_id).await
    }

    pub(crate) async fn create_entry(
        &self,
        user_id: &str,
        request: CreateGrowthEntryRequest,
    ) -> Result<GrowthEntryDto, UpstreamError> {
        self.growth.create_entry(user_id, request).await
    }

    pub(crate) async fn weekly_review(
        &self,
        user_id: &str,
    ) -> Result<WeeklyReviewDto, UpstreamError> {
        self.growth.weekly_review(user_id).await
    }

    pub(crate) async fn companion(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<CompanionBriefDto, UpstreamError> {
        self.growth.companion(user_id, local_date, timezone).await
    }

    pub(crate) async fn list_knowledge(
        &self,
        user_id: &str,
        request: KnowledgeQueryRequest,
    ) -> Result<Vec<KnowledgeResourceDto>, UpstreamError> {
        self.growth.list_knowledge(user_id, request).await
    }

    pub(crate) async fn create_knowledge(
        &self,
        user_id: &str,
        request: CreateKnowledgeResourceRequest,
        idempotency_key: Option<String>,
    ) -> Result<KnowledgeResourceDto, UpstreamError> {
        self.growth
            .create_knowledge(user_id, request, idempotency_key)
            .await
    }

    pub(crate) async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        request: UpdateKnowledgeResourceRequest,
    ) -> Result<KnowledgeResourceDto, UpstreamError> {
        self.growth
            .update_knowledge(user_id, resource_id, request)
            .await
    }

    pub(crate) async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError> {
        self.bbs_feed.feed(request).await
    }

    pub(crate) async fn get_content(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<ContentDto, UpstreamError> {
        self.public_content_for_viewer(user_id, id).await
    }

    pub(crate) async fn create_content(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError> {
        self.bbs_link
            .create(user_id, request, idempotency_key)
            .await
    }

    pub(crate) async fn update_content(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError> {
        self.bbs_link.update(user_id, id, request).await
    }

    pub(crate) async fn publish_content(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<ContentDto, UpstreamError> {
        self.bbs_link.publish(user_id, id).await
    }

    pub(crate) async fn search(
        &self,
        mut request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, UpstreamError> {
        let user_id = request
            .user_id
            .clone()
            .unwrap_or_else(|| "anonymous".to_string());
        let visibility = self.bbs.visibility_context(&user_id).await?;
        request.excluded_author_ids = normalized_excluded_author_ids(visibility);
        let mut response = self.search_main.search(request).await?;
        let route_ids = response
            .items
            .iter()
            .filter_map(|item| item.post.as_ref())
            .filter(|post| !post.route_title.trim().is_empty())
            .map(|post| post.id.clone())
            .collect::<Vec<_>>();
        if route_ids.is_empty() {
            return Ok(response);
        }
        match self.bbs.route_context(&user_id, route_ids).await {
            Ok(context) => {
                for post in response
                    .items
                    .iter_mut()
                    .filter_map(|item| item.post.as_mut())
                {
                    let live_count = context
                        .participant_counts
                        .get(&post.id)
                        .copied()
                        .unwrap_or_default();
                    post.join_count = post
                        .join_count
                        .saturating_add(u32::try_from(live_count).unwrap_or(u32::MAX));
                }
            }
            Err(error) => {
                response.degraded = true;
                tracing::warn!(%error, "search route context degraded");
            }
        }
        Ok(response)
    }

    pub(crate) async fn suggestions(
        &self,
        user_id: &str,
        query: &str,
    ) -> Result<SuggestionResponseDto, UpstreamError> {
        let visibility = self.bbs.visibility_context(user_id).await?;
        self.search_main
            .suggestions(SuggestionQueryRequest {
                q: query.to_string(),
                user_id: Some(user_id.to_string()),
                excluded_author_ids: normalized_excluded_author_ids(visibility),
            })
            .await
    }

    pub(crate) async fn ingest_events(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError> {
        self.user_event.ingest(user_id, request).await
    }

    pub(crate) async fn create_media_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError> {
        self.media.create_upload(user_id, request).await
    }

    pub(crate) async fn complete_media_upload(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<MediaDto, UpstreamError> {
        self.media.complete_upload(user_id, id).await
    }

    pub(crate) async fn get_media(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<MediaDto, UpstreamError> {
        self.media.get(user_id, id).await
    }

    pub(crate) async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError> {
        let content = self.public_content_for_viewer(user_id, post_id).await?;
        let reaction = self.like_status.reaction(user_id, post_id, request).await?;
        if reaction.reaction == bookway_api::ReactionTypeDto::Like
            && reaction.active
            && content.author_id != user_id
        {
            self.create_community_notification(
                &content.author_id,
                CreateUserNotificationRequest {
                    kind: NotificationKindDto::Community,
                    source_id: format!("like:{user_id}:{post_id}"),
                    title: "收到一个赞".to_string(),
                    body: "有人赞了你的内容".to_string(),
                    data: serde_json::json!({
                        "post_id": post_id,
                        "actor_id": user_id,
                        "interaction": "like",
                    }),
                },
            )
            .await;
        }
        Ok(reaction)
    }

    pub(crate) async fn social_context(
        &self,
        user_id: &str,
    ) -> Result<SocialContextDto, UpstreamError> {
        self.bbs.context(user_id).await
    }

    pub(crate) async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<RouteParticipationDto>, UpstreamError> {
        self.bbs.list_route_participations(user_id).await
    }

    pub(crate) async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        mut request: SetRouteParticipationRequest,
    ) -> Result<RouteParticipationStateDto, UpstreamError> {
        if !request.active {
            request.private_journey_id = None;
            let intent = self
                .growth
                .set_route_participation_intent(user_id, route_id, false, None)
                .await?;
            request.intent_version = Some(intent.version);
            let base_join_count = self
                .bbs_link
                .get_public(route_id)
                .await
                .ok()
                .map(|content| u64::from(content.post.join_count))
                .unwrap_or_default();
            let mut state = self
                .bbs
                .set_route_participation(user_id, route_id, request)
                .await?;
            state.participant_count = state.participant_count.saturating_add(base_join_count);
            return Ok(state);
        }
        let content = self.public_content_for_viewer(user_id, route_id).await?;
        validate_public_route(&content)?;
        if let Some(journey_id) = request.private_journey_id.as_deref() {
            let _ = self.growth.get_journey(user_id, journey_id).await?;
        }
        let intent = self
            .growth
            .set_route_participation_intent(
                user_id,
                route_id,
                true,
                request.private_journey_id.as_deref(),
            )
            .await?;
        request.intent_version = Some(intent.version);
        let mut state = self
            .bbs
            .set_route_participation(user_id, route_id, request)
            .await?;
        state.participant_count = state
            .participant_count
            .saturating_add(u64::from(content.post.join_count));
        Ok(state)
    }

    pub(crate) async fn join_route(
        &self,
        user_id: &str,
        route_id: &str,
    ) -> Result<RouteJoinResultDto, UpstreamError> {
        let content = self.public_content_for_viewer(user_id, route_id).await?;
        validate_public_route(&content)?;
        let journey = self
            .growth
            .create_route_journey(
                user_id,
                route_id,
                CreateJourneyRequest {
                    title: content.post.route_title.clone(),
                    intent: content.post.summary.clone(),
                    domain: content.post.domain,
                    journey_type: bookway_api::JourneyTypeDto::Project,
                    completion_criteria: "完成路线中的必要阶段和行动".to_string(),
                    stages: Vec::new(),
                    duration_label: if content.post.route_duration.trim().is_empty() {
                        "4 周".to_string()
                    } else {
                        content.post.route_duration.clone()
                    },
                    first_action_title: content.post.route_title.clone(),
                    first_action_detail: content.post.summary.clone(),
                    estimated_minutes: 20,
                    first_action_scheduled_label: None,
                    first_action_scheduled_for: None,
                    first_action_scheduled_timezone: None,
                    first_action_stage_index: None,
                    first_action_recurrence: None,
                },
            )
            .await?;
        let intent = self
            .growth
            .set_route_participation_intent(user_id, route_id, true, Some(&journey.id))
            .await?;
        let mut participation = self
            .bbs
            .set_route_participation(
                user_id,
                route_id,
                SetRouteParticipationRequest {
                    active: true,
                    private_journey_id: Some(journey.id.clone()),
                    intent_version: Some(intent.version),
                },
            )
            .await?;
        participation.participant_count = participation
            .participant_count
            .saturating_add(u64::from(content.post.join_count));
        Ok(RouteJoinResultDto {
            journey,
            participation,
        })
    }

    pub(crate) async fn report_content(
        &self,
        user_id: &str,
        content_id: &str,
        request: CreateContentReportRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentReportDto, UpstreamError> {
        let _ = self.public_content_for_viewer(user_id, content_id).await?;
        self.content_audit
            .report(user_id, content_id, request, idempotency_key)
            .await
    }

    pub(crate) async fn moderation_reports(
        &self,
        request: ContentReportQueryRequest,
    ) -> Result<ContentReportPageDto, UpstreamError> {
        self.content_audit.list_reports(request).await
    }

    pub(crate) async fn review_report(
        &self,
        reviewer_id: &str,
        report_id: &str,
        request: ReviewContentReportRequest,
    ) -> Result<ContentReportDto, UpstreamError> {
        let report = self
            .content_audit
            .review_report(reviewer_id, report_id, request)
            .await?;
        if report.action == ContentReportActionDto::RestrictContent {
            // This is a low-latency fast path. The review transaction also creates
            // a durable restriction job, so a transient content-service failure
            // cannot let an accepted moderation decision disappear.
            if let Err(error) = self.bbs_link.restrict(&report.content_id).await {
                tracing::warn!(
                    report_id,
                    content_id = %report.content_id,
                    %error,
                    "report restriction fast path failed; dispatcher will retry"
                );
            }
        }
        Ok(report)
    }

    pub(crate) async fn appeal_content(
        &self,
        user_id: &str,
        content_id: &str,
        request: CreateContentAppealRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentAppealDto, UpstreamError> {
        let content = self.bbs_link.get(content_id).await?;
        if content.author_id != user_id {
            return Err(UpstreamError::Grpc {
                service: "gateway",
                code: tonic::Code::PermissionDenied,
                message: "only the content author can appeal".to_string(),
            });
        }
        if content.status != bookway_api::ContentStatusDto::Restricted {
            return Err(UpstreamError::Grpc {
                service: "gateway",
                code: tonic::Code::FailedPrecondition,
                message: "only restricted content can be appealed".to_string(),
            });
        }
        self.content_audit
            .appeal(user_id, content_id, request, idempotency_key)
            .await
    }

    pub(crate) async fn own_contents(
        &self,
        user_id: &str,
        mut request: ContentQueryRequest,
    ) -> Result<ContentPageDto, UpstreamError> {
        // The author filter comes only from verified Gateway identity; callers
        // cannot turn this creator-management view into a content directory.
        request.author_id = Some(user_id.to_string());
        request.ids = None;
        self.bbs_link.list(request).await
    }

    pub(crate) async fn own_appeals(
        &self,
        user_id: &str,
        mut request: ContentAppealQueryRequest,
    ) -> Result<ContentAppealPageDto, UpstreamError> {
        // As with private content, ownership is imposed here rather than read
        // from client-provided query data.
        request.appellant_id = Some(user_id.to_string());
        request.content_id = None;
        self.content_audit.list_appeals(request).await
    }

    pub(crate) async fn moderation_appeals(
        &self,
        mut request: ContentAppealQueryRequest,
    ) -> Result<ContentAppealPageDto, UpstreamError> {
        request
            .status
            .get_or_insert(bookway_api::ContentAppealStatusDto::Pending);
        self.content_audit.list_appeals(request).await
    }

    pub(crate) async fn review_appeal(
        &self,
        reviewer_id: &str,
        appeal_id: &str,
        request: ReviewContentAppealRequest,
    ) -> Result<ContentAppealDto, UpstreamError> {
        let appeal = self
            .content_audit
            .review_appeal(reviewer_id, appeal_id, request)
            .await?;
        if appeal.action == ContentReportActionDto::RestoreContent {
            // This is a low-latency fast path only. The audit transaction has
            // already stored a durable worker task, so a failed handoff cannot
            // leave a terminal appeal without eventual restoration.
            if let Err(error) = self.bbs_link.restore(&appeal.content_id).await {
                tracing::warn!(
                    appeal_id,
                    content_id = %appeal.content_id,
                    %error,
                    "appeal restore fast path failed; dispatcher will retry"
                );
            }
        }
        Ok(appeal)
    }

    pub(crate) async fn comments(
        &self,
        user_id: &str,
        post_id: &str,
        mut request: CommentQueryRequest,
    ) -> Result<CommentPageDto, UpstreamError> {
        let (_, excluded_author_ids) = self
            .public_content_with_visibility(user_id, post_id)
            .await?;
        request.excluded_author_ids = excluded_author_ids;
        self.comment.comments(post_id, request).await
    }

    pub(crate) async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        mut request: CreateCommentRequest,
        idempotency_key: Option<String>,
    ) -> Result<CommentDto, UpstreamError> {
        let (content, excluded_author_ids) = self
            .public_content_with_visibility(user_id, post_id)
            .await?;
        request.excluded_author_ids = excluded_author_ids;
        let result = self
            .comment
            .create_comment(user_id, post_id, request, idempotency_key)
            .await?;
        for (recipient_user_id, notification) in
            community_comment_notifications(&content.author_id, &result, user_id, post_id)
        {
            self.create_community_notification(&recipient_user_id, notification)
                .await;
        }
        Ok(result.comment)
    }

    pub(crate) async fn delete_comment(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), UpstreamError> {
        self.comment
            .delete_comment(user_id, post_id, comment_id)
            .await
    }

    pub(crate) async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError> {
        let creates_follow_notification = request.edge == SocialEdgeTypeDto::Follow
            && request.active
            && target_user_id != user_id;
        let context = self.bbs.follow(user_id, target_user_id, request).await?;
        if creates_follow_notification {
            self.create_community_notification(
                target_user_id,
                CreateUserNotificationRequest {
                    kind: NotificationKindDto::Community,
                    source_id: format!("follow:{user_id}:{target_user_id}"),
                    title: "收到一个新关注".to_string(),
                    body: "有人开始关注你".to_string(),
                    data: serde_json::json!({
                        "actor_id": user_id,
                        "target_user_id": target_user_id,
                        "interaction": "follow",
                    }),
                },
            )
            .await;
        }
        Ok(context)
    }

    async fn create_community_notification(
        &self,
        recipient_user_id: &str,
        request: CreateUserNotificationRequest,
    ) {
        let source_id = request.source_id.clone();
        if let Err(error) = self
            .growth
            .create_notification(recipient_user_id, request)
            .await
        {
            tracing::warn!(
                %error,
                %source_id,
                %recipient_user_id,
                "community notification degraded"
            );
        }
    }

    /// Public-content status alone is not sufficient for a viewer-scoped
    /// surface: a direct URL must obey the same social policy as feed/search.
    async fn public_content_for_viewer(
        &self,
        user_id: &str,
        content_id: &str,
    ) -> Result<ContentDto, UpstreamError> {
        Ok(self
            .public_content_with_visibility(user_id, content_id)
            .await?
            .0)
    }

    async fn public_content_with_visibility(
        &self,
        user_id: &str,
        content_id: &str,
    ) -> Result<(ContentDto, Vec<String>), UpstreamError> {
        let content = self.bbs_link.get_public(content_id).await?;
        let excluded_author_ids =
            normalized_excluded_author_ids(self.bbs.visibility_context(user_id).await?);
        if excluded_author_ids
            .iter()
            .any(|author_id| author_id == &content.author_id)
        {
            return Err(hidden_content());
        }
        Ok((content, excluded_author_ids))
    }
}

fn route_precondition(message: &str) -> UpstreamError {
    UpstreamError::Grpc {
        service: "gateway",
        code: tonic::Code::FailedPrecondition,
        message: message.to_string(),
    }
}

fn hidden_content() -> UpstreamError {
    UpstreamError::Grpc {
        service: "gateway",
        code: tonic::Code::NotFound,
        // Keep the response indistinguishable from an unpublished or absent ID.
        message: "content not found".to_string(),
    }
}

fn validate_public_route(content: &ContentDto) -> Result<(), UpstreamError> {
    if content.status != bookway_api::ContentStatusDto::Published {
        return Err(route_precondition("只能加入已公开发布的路线"));
    }
    if content.post.route_title.trim().is_empty() {
        return Err(route_precondition("该内容没有可加入的路线"));
    }
    Ok(())
}

fn normalized_excluded_author_ids(visibility: SocialVisibilityDto) -> Vec<String> {
    visibility
        .excluded_author_ids
        .into_iter()
        .map(|author_id| author_id.trim().to_string())
        .filter(|author_id| !author_id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn community_comment_notifications(
    post_author_id: &str,
    result: &CreateCommentResult,
    actor_id: &str,
    post_id: &str,
) -> Vec<(String, CreateUserNotificationRequest)> {
    let comment = &result.comment;
    if comment.status != bookway_api::ContentStatusDto::Published {
        return Vec::new();
    }

    let mut notifications = Vec::with_capacity(2);
    if post_author_id != actor_id {
        notifications.push((
            post_author_id.to_string(),
            CreateUserNotificationRequest {
                kind: NotificationKindDto::Community,
                source_id: format!("comment-post:{}:{post_author_id}", comment.id),
                title: "收到一条评论".to_string(),
                body: "有人评论了你的内容".to_string(),
                data: serde_json::json!({
                    "post_id": post_id,
                    "comment_id": comment.id.clone(),
                    "actor_id": actor_id,
                    "interaction": "comment",
                }),
            },
        ));
    }
    if let Some(parent_author_id) = result.parent_author_id.as_deref()
        && parent_author_id != actor_id
        && parent_author_id != post_author_id
    {
        notifications.push((
            parent_author_id.to_string(),
            CreateUserNotificationRequest {
                kind: NotificationKindDto::Community,
                source_id: format!("comment-reply:{}:{parent_author_id}", comment.id),
                title: "收到一条回复".to_string(),
                body: "有人回复了你的评论".to_string(),
                data: serde_json::json!({
                    "post_id": post_id,
                    "comment_id": comment.id.clone(),
                    "parent_comment_id": comment.parent_id.clone(),
                    "actor_id": actor_id,
                    "interaction": "comment_reply",
                }),
            },
        ));
    }
    notifications
}

#[cfg(test)]
mod tests {
    use super::{community_comment_notifications, normalized_excluded_author_ids};
    use bookway_api::{CommentDto, ContentStatusDto, CreateCommentResult, SocialVisibilityDto};

    #[test]
    fn search_visibility_canonicalizes_internal_author_exclusions() {
        let excluded = normalized_excluded_author_ids(SocialVisibilityDto {
            excluded_author_ids: vec![
                "author-b".to_string(),
                " author-b ".to_string(),
                "author-a".to_string(),
                " ".to_string(),
                "author-c".to_string(),
            ],
        });

        assert_eq!(excluded, vec!["author-a", "author-b", "author-c"]);
    }

    #[test]
    fn reply_notifies_post_and_parent_authors_with_distinct_stable_sources() {
        let notifications = community_comment_notifications(
            "post-author",
            &reply_result("parent-author", ContentStatusDto::Published),
            "reply-author",
            "post-a",
        );

        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].0, "post-author");
        assert_eq!(
            notifications[0].1.source_id,
            "comment-post:comment-a:post-author"
        );
        assert_eq!(notifications[0].1.data["interaction"], "comment");
        assert_eq!(notifications[1].0, "parent-author");
        assert_eq!(
            notifications[1].1.source_id,
            "comment-reply:comment-a:parent-author"
        );
        assert_eq!(notifications[1].1.data["interaction"], "comment_reply");
    }

    #[test]
    fn reply_notification_avoids_duplicate_and_self_notifications() {
        let same_recipient = community_comment_notifications(
            "post-and-parent-author",
            &reply_result("post-and-parent-author", ContentStatusDto::Published),
            "reply-author",
            "post-a",
        );
        assert_eq!(same_recipient.len(), 1);
        assert_eq!(same_recipient[0].0, "post-and-parent-author");

        let actor_is_post_author = community_comment_notifications(
            "reply-author",
            &reply_result("parent-author", ContentStatusDto::Published),
            "reply-author",
            "post-a",
        );
        assert_eq!(actor_is_post_author.len(), 1);
        assert_eq!(actor_is_post_author[0].0, "parent-author");

        let unpublished = community_comment_notifications(
            "post-author",
            &reply_result("parent-author", ContentStatusDto::Reviewing),
            "reply-author",
            "post-a",
        );
        assert!(unpublished.is_empty());
    }

    fn reply_result(parent_author_id: &str, status: ContentStatusDto) -> CreateCommentResult {
        CreateCommentResult {
            comment: CommentDto {
                id: "comment-a".to_string(),
                post_id: "post-a".to_string(),
                author_id: "reply-author".to_string(),
                author_name: "reply-author".to_string(),
                body: "回复".to_string(),
                parent_id: Some("parent-comment-a".to_string()),
                like_count: 0,
                created_at: "2026-08-15T00:00:00Z".to_string(),
                status,
            },
            parent_author_id: Some(parent_author_id.to_string()),
        }
    }
}
