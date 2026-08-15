use async_trait::async_trait;
use thiserror::Error;
use tonic::{Request, transport::Channel};

use super::api::{
    ActionDto, CommentDto, CommentPageDto, CommentQueryRequest, CompanionBriefDto,
    ContentAppealDto, ContentAppealPageDto, ContentAppealQueryRequest, ContentDto, ContentPageDto,
    ContentQueryRequest, ContentReportDto, ContentReportPageDto, ContentReportQueryRequest,
    CreateActionRequest, CreateCommentRequest, CreateCommentResult, CreateContentAppealRequest,
    CreateContentReportRequest, CreateContentRequest, CreateGrowthEntryRequest,
    CreateJourneyRequest, CreateKnowledgeResourceRequest, CreateUserNotificationRequest, FeedDto,
    FeedQueryRequest, FollowRequest, GrowthEntryDto, JourneyDetailDto, JourneyDto,
    KnowledgeQueryRequest, KnowledgeResourceDto, MediaDto, MediaUploadRequest, MediaUploadResponse,
    NotificationPageDto, NotificationQueryRequest, PushDeviceDto, ReactionDto, ReactionRequest,
    RegisterPushDeviceRequest, ReminderPreferencesDto, ReviewContentAppealRequest,
    ReviewContentReportRequest, RouteParticipationContextDto, RouteParticipationDto,
    RouteParticipationIntentDto, RouteParticipationStateDto, SearchQueryRequest, SearchResponseDto,
    SetRouteParticipationRequest, SocialContextDto, SocialVisibilityDto, SuggestionQueryRequest,
    SuggestionResponseDto, TodayDto, UpdateActionRequest, UpdateContentRequest,
    UpdateJourneyRequest, UpdateKnowledgeResourceRequest, UpdateReminderPreferencesRequest,
    UserEventBatchRequest, UserEventIngestResponse, UserNotificationDto, WeeklyReviewDto,
};

#[derive(Debug, Error)]
pub(crate) enum UpstreamError {
    #[error("{service} grpc request failed: {message}")]
    Transport {
        service: &'static str,
        message: String,
    },
    #[error("{service} grpc request failed with {code:?}: {message}")]
    Grpc {
        service: &'static str,
        code: tonic::Code,
        message: String,
    },
}

#[async_trait]
pub(crate) trait BbsFeedDataSource: Send + Sync {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait BbsLinkDataSource: Send + Sync {
    async fn list(&self, request: ContentQueryRequest) -> Result<ContentPageDto, UpstreamError>;
    async fn get(&self, id: &str) -> Result<ContentDto, UpstreamError>;
    async fn get_public(&self, id: &str) -> Result<ContentDto, UpstreamError>;
    async fn create(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError>;
    async fn update(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError>;
    async fn publish(&self, user_id: &str, id: &str) -> Result<ContentDto, UpstreamError>;
    async fn restrict(&self, id: &str) -> Result<ContentDto, UpstreamError>;
    async fn restore(&self, id: &str) -> Result<ContentDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait SearchMainDataSource: Send + Sync {
    async fn search(&self, request: SearchQueryRequest)
    -> Result<SearchResponseDto, UpstreamError>;
    async fn suggestions(
        &self,
        request: SuggestionQueryRequest,
    ) -> Result<SuggestionResponseDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait UserEventDataSource: Send + Sync {
    async fn ingest(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError>;
}

#[async_trait]
pub(crate) trait MediaDataSource: Send + Sync {
    async fn create_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError>;
    async fn complete_upload(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError>;
    async fn get(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait BbsDataSource: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, UpstreamError>;
    async fn visibility_context(&self, user_id: &str)
    -> Result<SocialVisibilityDto, UpstreamError>;
    async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError>;
    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<RouteParticipationDto>, UpstreamError>;
    async fn route_context(
        &self,
        user_id: &str,
        route_ids: Vec<String>,
    ) -> Result<RouteParticipationContextDto, UpstreamError>;
    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        request: SetRouteParticipationRequest,
    ) -> Result<RouteParticipationStateDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait CommentDataSource: Send + Sync {
    async fn comments(
        &self,
        post_id: &str,
        request: CommentQueryRequest,
    ) -> Result<CommentPageDto, UpstreamError>;
    async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
        idempotency_key: Option<String>,
    ) -> Result<CreateCommentResult, UpstreamError>;
    async fn delete_comment(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), UpstreamError>;
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum CommentListResponse {
    Page(CommentPageDto),
    Legacy(Vec<CommentDto>),
}

#[async_trait]
pub(crate) trait LikeStatusDataSource: Send + Sync {
    async fn reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait ContentAuditDataSource: Send + Sync {
    async fn report(
        &self,
        user_id: &str,
        content_id: &str,
        request: CreateContentReportRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentReportDto, UpstreamError>;
    async fn list_reports(
        &self,
        request: ContentReportQueryRequest,
    ) -> Result<ContentReportPageDto, UpstreamError>;
    async fn review_report(
        &self,
        reviewer_id: &str,
        report_id: &str,
        request: ReviewContentReportRequest,
    ) -> Result<ContentReportDto, UpstreamError>;
    async fn appeal(
        &self,
        user_id: &str,
        content_id: &str,
        request: CreateContentAppealRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentAppealDto, UpstreamError>;
    async fn list_appeals(
        &self,
        request: ContentAppealQueryRequest,
    ) -> Result<ContentAppealPageDto, UpstreamError>;
    async fn review_appeal(
        &self,
        reviewer_id: &str,
        appeal_id: &str,
        request: ReviewContentAppealRequest,
    ) -> Result<ContentAppealDto, UpstreamError>;
}

enum Client {
    Growth(bookway_growth::api::pb::growth_client::GrowthClient<Channel>),
    BbsFeed(bookway_bbs_feed::api::pb::bbs_feed_client::BbsFeedClient<Channel>),
    BbsLink(bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient<Channel>),
    SearchMain(bookway_search_main::api::pb::search_main_client::SearchMainClient<Channel>),
    Bbs(bookway_bbs::api::pb::bbs_client::BbsClient<Channel>),
    Comment(bookway_comment::api::pb::comment_client::CommentClient<Channel>),
    LikeStatus(
        bookway_commonlikestatus::api::pb::common_like_status_client::CommonLikeStatusClient<
            Channel,
        >,
    ),
    UserEvent(bookway_user_event::api::pb::user_event_client::UserEventClient<Channel>),
    Media(bookway_media::api::pb::media_client::MediaClient<Channel>),
    ContentAudit(bookway_content_audit::api::pb::content_audit_client::ContentAuditClient<Channel>),
}

pub(crate) struct GrpcDataSource {
    client: Client,
}

impl GrpcDataSource {
    pub(crate) async fn connect(
        service: &'static str,
        address: String,
    ) -> Result<Self, tonic::transport::Error> {
        let client = match service {
            "growth" => Client::Growth(
                bookway_growth::api::pb::growth_client::GrowthClient::connect(address).await?,
            ),
            "bbs-feed" => Client::BbsFeed(
                bookway_bbs_feed::api::pb::bbs_feed_client::BbsFeedClient::connect(address)
                    .await?,
            ),
            "bbs-link" => Client::BbsLink(
                bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient::connect(address)
                    .await?,
            ),
            "search-main" => Client::SearchMain(
                bookway_search_main::api::pb::search_main_client::SearchMainClient::connect(
                    address,
                )
                .await?,
            ),
            "bbs" => Client::Bbs(
                bookway_bbs::api::pb::bbs_client::BbsClient::connect(address).await?,
            ),
            "comment" => Client::Comment(
                bookway_comment::api::pb::comment_client::CommentClient::connect(address).await?,
            ),
            "commonlikestatus" => Client::LikeStatus(
                bookway_commonlikestatus::api::pb::common_like_status_client::CommonLikeStatusClient::connect(address).await?,
            ),
            "user-event" => Client::UserEvent(
                bookway_user_event::api::pb::user_event_client::UserEventClient::connect(address)
                    .await?,
            ),
            "media" => Client::Media(
                bookway_media::api::pb::media_client::MediaClient::connect(address).await?,
            ),
            "content-audit" => Client::ContentAudit(
                bookway_content_audit::api::pb::content_audit_client::ContentAuditClient::connect(
                    address,
                )
                .await?,
            ),
            _ => panic!("unsupported gateway grpc service: {service}"),
        };
        Ok(Self { client })
    }
}

fn encode<T: serde::Serialize>(service: &'static str, value: &T) -> Result<String, UpstreamError> {
    serde_json::to_string(value).map_err(|error| UpstreamError::Transport {
        service,
        message: error.to_string(),
    })
}

fn decode<T: serde::de::DeserializeOwned>(
    service: &'static str,
    value: String,
) -> Result<T, UpstreamError> {
    serde_json::from_str(&value).map_err(|error| UpstreamError::Transport {
        service,
        message: error.to_string(),
    })
}

fn status<T>(service: &'static str, result: Result<T, tonic::Status>) -> Result<T, UpstreamError> {
    result.map_err(|error| UpstreamError::Grpc {
        service,
        code: error.code(),
        message: error.to_string(),
    })
}

fn privileged_request<T>(service: &'static str, message: T) -> Result<Request<T>, UpstreamError> {
    bookway_runtime::grpc_service_request(message).map_err(|error| UpstreamError::Transport {
        service,
        message: error.to_string(),
    })
}

impl GrpcDataSource {
    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<JourneyDto>, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .list_journeys(bookway_growth::api::pb::UserRequest {
                    user_id: user_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_journey(bookway_growth::api::pb::CreateJourneyRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_route_journey(
        &self,
        user_id: &str,
        route_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_route_journey(bookway_growth::api::pb::CreateRouteJourneyRequest {
                    user_id: user_id.to_string(),
                    route_id: route_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn set_route_participation_intent(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<&str>,
    ) -> Result<RouteParticipationIntentDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .set_route_participation_intent(
                    bookway_growth::api::pb::SetRouteParticipationIntentRequest {
                        user_id: user_id.to_string(),
                        route_id: route_id.to_string(),
                        active,
                        private_journey_id: private_journey_id.unwrap_or_default().to_string(),
                    },
                )
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .get_journey(bookway_growth::api::pb::JourneyRequest {
                    user_id: user_id.to_string(),
                    journey_id: journey_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .update_journey(bookway_growth::api::pb::UpdateJourneyRequest {
                    user_id: user_id.to_string(),
                    journey_id: journey_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_action(
        &self,
        user_id: &str,
        request: CreateActionRequest,
    ) -> Result<ActionDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_action(bookway_growth::api::pb::CreateActionRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn today(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<TodayDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .today(bookway_growth::api::pb::ScheduleRequest {
                    user_id: user_id.to_string(),
                    local_date: local_date.unwrap_or_default().to_string(),
                    timezone: timezone.unwrap_or_default().to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .complete_action(bookway_growth::api::pb::CompleteActionRequest {
                    user_id: user_id.to_string(),
                    action_id: action_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .update_action(bookway_growth::api::pb::UpdateActionRequest {
                    user_id: user_id.to_string(),
                    action_id: action_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn reminder_preferences(
        &self,
        user_id: &str,
    ) -> Result<ReminderPreferencesDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .reminder_preferences(bookway_growth::api::pb::UserRequest {
                    user_id: user_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn update_reminder_preferences(
        &self,
        user_id: &str,
        request: UpdateReminderPreferencesRequest,
    ) -> Result<ReminderPreferencesDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .update_reminder_preferences(
                    bookway_growth::api::pb::UpdateReminderPreferencesRequest {
                        user_id: user_id.to_string(),
                        request_json: encode("growth", &request)?,
                    },
                )
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn register_push_device(
        &self,
        user_id: &str,
        request: RegisterPushDeviceRequest,
    ) -> Result<PushDeviceDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .register_push_device(bookway_growth::api::pb::RegisterPushDeviceRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn revoke_push_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .revoke_push_device(bookway_growth::api::pb::PushDeviceRequest {
                    user_id: user_id.to_string(),
                    device_id: device_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        let _: serde_json::Value = decode("growth", response.response_json)?;
        Ok(())
    }

    pub(crate) async fn list_notifications(
        &self,
        user_id: &str,
        request: NotificationQueryRequest,
    ) -> Result<NotificationPageDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .list_notifications(bookway_growth::api::pb::NotificationQueryRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_notification(
        &self,
        user_id: &str,
        request: CreateUserNotificationRequest,
    ) -> Result<UserNotificationDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_notification(bookway_growth::api::pb::CreateNotificationRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<UserNotificationDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .mark_notification_read(bookway_growth::api::pb::NotificationRequest {
                    user_id: user_id.to_string(),
                    notification_id: notification_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn list_entries(
        &self,
        user_id: &str,
    ) -> Result<Vec<GrowthEntryDto>, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .list_entries(bookway_growth::api::pb::UserRequest {
                    user_id: user_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_entry(
        &self,
        user_id: &str,
        request: CreateGrowthEntryRequest,
    ) -> Result<GrowthEntryDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_entry(bookway_growth::api::pb::CreateEntryRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn weekly_review(
        &self,
        user_id: &str,
    ) -> Result<WeeklyReviewDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .weekly_review(bookway_growth::api::pb::UserRequest {
                    user_id: user_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn companion(
        &self,
        user_id: &str,
        local_date: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<CompanionBriefDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .companion(bookway_growth::api::pb::ScheduleRequest {
                    user_id: user_id.to_string(),
                    local_date: local_date.unwrap_or_default().to_string(),
                    timezone: timezone.unwrap_or_default().to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn list_knowledge(
        &self,
        user_id: &str,
        request: KnowledgeQueryRequest,
    ) -> Result<Vec<KnowledgeResourceDto>, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .list_knowledge(bookway_growth::api::pb::KnowledgeQueryRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_knowledge(
        &self,
        user_id: &str,
        request: CreateKnowledgeResourceRequest,
        idempotency_key: Option<String>,
    ) -> Result<KnowledgeResourceDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_knowledge(bookway_growth::api::pb::CreateKnowledgeRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                    idempotency_key: idempotency_key.unwrap_or_default(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn update_knowledge(
        &self,
        user_id: &str,
        resource_id: &str,
        request: UpdateKnowledgeResourceRequest,
    ) -> Result<KnowledgeResourceDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .update_knowledge(bookway_growth::api::pb::UpdateKnowledgeRequest {
                    user_id: user_id.to_string(),
                    resource_id: resource_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }
}

#[async_trait]
impl BbsFeedDataSource for GrpcDataSource {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError> {
        let Client::BbsFeed(client) = &self.client else {
            return Err(wrong_service("bbs-feed"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-feed",
            client
                .feed(bookway_bbs_feed::api::pb::FeedRequest {
                    request_json: encode("bbs-feed", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("bbs-feed", response.response_json)
    }
}

#[async_trait]
impl BbsLinkDataSource for GrpcDataSource {
    async fn list(&self, request: ContentQueryRequest) -> Result<ContentPageDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .list(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::ListRequest {
                        request_json: encode("bbs-link", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn get(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .get(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::IdRequest { id: id.to_string() },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn get_public(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .get_public(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::IdRequest { id: id.to_string() },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn create(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .create(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::CreateRequest {
                        user_id: user_id.to_string(),
                        request_json: encode("bbs-link", &request)?,
                        idempotency_key: idempotency_key.unwrap_or_default(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn update(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .update(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::UpdateRequest {
                        user_id: user_id.to_string(),
                        id: id.to_string(),
                        request_json: encode("bbs-link", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn publish(&self, user_id: &str, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .publish(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::PublishRequest {
                        user_id: user_id.to_string(),
                        id: id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn restrict(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .restrict(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::RestrictRequest {
                        content_id: id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn restore(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .restore(privileged_request(
                    "bbs-link",
                    bookway_bbs_link::api::pb::RestoreRequest {
                        content_id: id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }
}

#[async_trait]
impl SearchMainDataSource for GrpcDataSource {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, UpstreamError> {
        let Client::SearchMain(client) = &self.client else {
            return Err(wrong_service("search-main"));
        };
        let mut client = client.clone();
        let response = status(
            "search-main",
            client
                .search(bookway_search_main::api::pb::SearchRequest {
                    request_json: encode("search-main", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("search-main", response.response_json)
    }

    async fn suggestions(
        &self,
        request: SuggestionQueryRequest,
    ) -> Result<SuggestionResponseDto, UpstreamError> {
        let Client::SearchMain(client) = &self.client else {
            return Err(wrong_service("search-main"));
        };
        let mut client = client.clone();
        let response = status(
            "search-main",
            client
                .suggestions(bookway_search_main::api::pb::SuggestionsRequest {
                    // Keep the legacy text blank so an old search-main instance
                    // returns no suggestions rather than bypassing visibility.
                    query: String::new(),
                    request_json: encode("search-main", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("search-main", response.response_json)
    }
}

#[async_trait]
impl UserEventDataSource for GrpcDataSource {
    async fn ingest(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError> {
        let Client::UserEvent(client) = &self.client else {
            return Err(wrong_service("user-event"));
        };
        let mut client = client.clone();
        let response = status(
            "user-event",
            client
                .ingest(bookway_user_event::api::pb::IngestRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("user-event", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("user-event", response.response_json)
    }
}

#[async_trait]
impl MediaDataSource for GrpcDataSource {
    async fn create_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError> {
        let Client::Media(client) = &self.client else {
            return Err(wrong_service("media"));
        };
        let mut client = client.clone();
        let response = status(
            "media",
            client
                .create_upload(bookway_media::api::pb::CreateUploadRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("media", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("media", response.response_json)
    }

    async fn complete_upload(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError> {
        let Client::Media(client) = &self.client else {
            return Err(wrong_service("media"));
        };
        let mut client = client.clone();
        let response = status(
            "media",
            client
                .complete_upload(bookway_media::api::pb::ResourceRequest {
                    user_id: user_id.to_string(),
                    id: id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("media", response.response_json)
    }

    async fn get(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError> {
        let Client::Media(client) = &self.client else {
            return Err(wrong_service("media"));
        };
        let mut client = client.clone();
        let response = status(
            "media",
            client
                .get(bookway_media::api::pb::ResourceRequest {
                    user_id: user_id.to_string(),
                    id: id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("media", response.response_json)
    }
}

#[async_trait]
impl BbsDataSource for GrpcDataSource {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .context(privileged_request(
                    "bbs",
                    bookway_bbs::api::pb::ContextRequest {
                        user_id: user_id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<SocialVisibilityDto, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .visibility_context(privileged_request(
                    "bbs",
                    bookway_bbs::api::pb::ContextRequest {
                        user_id: user_id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }

    async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .set_edge(privileged_request(
                    "bbs",
                    bookway_bbs::api::pb::SetEdgeRequest {
                        user_id: user_id.to_string(),
                        target_user_id: target_user_id.to_string(),
                        request_json: encode("bbs", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<RouteParticipationDto>, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .list_route_participations(privileged_request(
                    "bbs",
                    bookway_bbs::api::pb::ContextRequest {
                        user_id: user_id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: Vec<String>,
    ) -> Result<RouteParticipationContextDto, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .route_context(privileged_request(
                    "bbs",
                    bookway_bbs::api::pb::RouteContextRequest {
                        user_id: user_id.to_string(),
                        route_ids,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        request: SetRouteParticipationRequest,
    ) -> Result<RouteParticipationStateDto, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .set_route_participation(privileged_request(
                    "bbs",
                    bookway_bbs::api::pb::RouteParticipationRequest {
                        user_id: user_id.to_string(),
                        route_id: route_id.to_string(),
                        request_json: encode("bbs", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }
}

#[async_trait]
impl CommentDataSource for GrpcDataSource {
    async fn comments(
        &self,
        post_id: &str,
        request: CommentQueryRequest,
    ) -> Result<CommentPageDto, UpstreamError> {
        let Client::Comment(client) = &self.client else {
            return Err(wrong_service("comment"));
        };
        let mut client = client.clone();
        let response = status(
            "comment",
            client
                .list(privileged_request(
                    "comment",
                    bookway_comment::api::pb::ListRequest {
                        post_id: post_id.to_string(),
                        request_json: encode("comment", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        match decode("comment", response.response_json)? {
            CommentListResponse::Page(page) => Ok(page),
            CommentListResponse::Legacy(items) => Ok(CommentPageDto {
                items,
                next_cursor: None,
            }),
        }
    }

    async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
        idempotency_key: Option<String>,
    ) -> Result<CreateCommentResult, UpstreamError> {
        let Client::Comment(client) = &self.client else {
            return Err(wrong_service("comment"));
        };
        let mut client = client.clone();
        let response = status(
            "comment",
            client
                .create(privileged_request(
                    "comment",
                    bookway_comment::api::pb::CreateRequest {
                        user_id: user_id.to_string(),
                        post_id: post_id.to_string(),
                        request_json: encode("comment", &request)?,
                        idempotency_key,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("comment", response.response_json)
    }

    async fn delete_comment(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), UpstreamError> {
        let Client::Comment(client) = &self.client else {
            return Err(wrong_service("comment"));
        };
        let mut client = client.clone();
        let response = status(
            "comment",
            client
                .delete(privileged_request(
                    "comment",
                    bookway_comment::api::pb::DeleteRequest {
                        user_id: user_id.to_string(),
                        post_id: post_id.to_string(),
                        comment_id: comment_id.to_string(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        let _: () = decode("comment", response.response_json)?;
        Ok(())
    }
}

#[async_trait]
impl LikeStatusDataSource for GrpcDataSource {
    async fn reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError> {
        let Client::LikeStatus(client) = &self.client else {
            return Err(wrong_service("commonlikestatus"));
        };
        let mut client = client.clone();
        let response = status(
            "commonlikestatus",
            client
                .set_reaction(bookway_commonlikestatus::api::pb::SetReactionRequest {
                    user_id: user_id.to_string(),
                    post_id: post_id.to_string(),
                    request_json: encode("commonlikestatus", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("commonlikestatus", response.response_json)
    }
}

#[async_trait]
impl ContentAuditDataSource for GrpcDataSource {
    async fn report(
        &self,
        user_id: &str,
        content_id: &str,
        request: CreateContentReportRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentReportDto, UpstreamError> {
        let Client::ContentAudit(client) = &self.client else {
            return Err(wrong_service("content-audit"));
        };
        let mut client = client.clone();
        let response = status(
            "content-audit",
            client
                .report(privileged_request(
                    "content-audit",
                    bookway_content_audit::api::pb::ReportRequest {
                        reporter_id: user_id.to_string(),
                        content_id: content_id.to_string(),
                        request_json: encode("content-audit", &request)?,
                        idempotency_key: idempotency_key.unwrap_or_default(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("content-audit", response.response_json)
    }

    async fn list_reports(
        &self,
        request: ContentReportQueryRequest,
    ) -> Result<ContentReportPageDto, UpstreamError> {
        let Client::ContentAudit(client) = &self.client else {
            return Err(wrong_service("content-audit"));
        };
        let mut client = client.clone();
        let response = status(
            "content-audit",
            client
                .list_reports(privileged_request(
                    "content-audit",
                    bookway_content_audit::api::pb::ListReportsRequest {
                        request_json: encode("content-audit", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("content-audit", response.response_json)
    }

    async fn review_report(
        &self,
        reviewer_id: &str,
        report_id: &str,
        request: ReviewContentReportRequest,
    ) -> Result<ContentReportDto, UpstreamError> {
        let Client::ContentAudit(client) = &self.client else {
            return Err(wrong_service("content-audit"));
        };
        let mut client = client.clone();
        let response = status(
            "content-audit",
            client
                .review_report(privileged_request(
                    "content-audit",
                    bookway_content_audit::api::pb::ReviewReportRequest {
                        reviewer_id: reviewer_id.to_string(),
                        report_id: report_id.to_string(),
                        request_json: encode("content-audit", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("content-audit", response.response_json)
    }

    async fn appeal(
        &self,
        user_id: &str,
        content_id: &str,
        request: CreateContentAppealRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentAppealDto, UpstreamError> {
        let Client::ContentAudit(client) = &self.client else {
            return Err(wrong_service("content-audit"));
        };
        let mut client = client.clone();
        let response = status(
            "content-audit",
            client
                .appeal(privileged_request(
                    "content-audit",
                    bookway_content_audit::api::pb::AppealRequest {
                        appellant_id: user_id.to_string(),
                        content_id: content_id.to_string(),
                        request_json: encode("content-audit", &request)?,
                        idempotency_key: idempotency_key.unwrap_or_default(),
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("content-audit", response.response_json)
    }

    async fn list_appeals(
        &self,
        request: ContentAppealQueryRequest,
    ) -> Result<ContentAppealPageDto, UpstreamError> {
        let Client::ContentAudit(client) = &self.client else {
            return Err(wrong_service("content-audit"));
        };
        let mut client = client.clone();
        let response = status(
            "content-audit",
            client
                .list_appeals(privileged_request(
                    "content-audit",
                    bookway_content_audit::api::pb::ListAppealsRequest {
                        request_json: encode("content-audit", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("content-audit", response.response_json)
    }

    async fn review_appeal(
        &self,
        reviewer_id: &str,
        appeal_id: &str,
        request: ReviewContentAppealRequest,
    ) -> Result<ContentAppealDto, UpstreamError> {
        let Client::ContentAudit(client) = &self.client else {
            return Err(wrong_service("content-audit"));
        };
        let mut client = client.clone();
        let response = status(
            "content-audit",
            client
                .review_appeal(privileged_request(
                    "content-audit",
                    bookway_content_audit::api::pb::ReviewAppealRequest {
                        reviewer_id: reviewer_id.to_string(),
                        appeal_id: appeal_id.to_string(),
                        request_json: encode("content-audit", &request)?,
                    },
                )?)
                .await,
        )?
        .into_inner();
        decode("content-audit", response.response_json)
    }
}

fn wrong_service(service: &'static str) -> UpstreamError {
    UpstreamError::Transport {
        service,
        message: "client type does not match datasource contract".to_string(),
    }
}
