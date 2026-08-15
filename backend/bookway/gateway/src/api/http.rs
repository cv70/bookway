use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};
use serde::Deserialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use super::{
    ActionDto, CommentDto, CommentPageDto, CommentQueryRequest, CompanionBriefDto,
    ContentAppealDto, ContentAppealPageDto, ContentAppealQueryRequest, ContentDto, ContentPageDto,
    ContentQueryRequest, ContentReportDto, ContentReportPageDto, ContentReportQueryRequest,
    CreateActionRequest, CreateCommentRequest, CreateContentAppealRequest,
    CreateContentReportRequest, CreateContentRequest, CreateGrowthEntryRequest,
    CreateJourneyRequest, CreateKnowledgeResourceRequest, FeedDto, FeedQueryRequest, FollowRequest,
    GrowthEntryDto, JourneyDetailDto, JourneyDto, KnowledgeQueryRequest, KnowledgeResourceDto,
    MediaDto, MediaUploadRequest, MediaUploadResponse, NotificationPageDto,
    NotificationQueryRequest, PushDeviceDto, ReactionDto, ReactionRequest,
    RegisterPushDeviceRequest, ReminderPreferencesDto, ReviewContentAppealRequest,
    ReviewContentReportRequest, RouteJoinResultDto, RouteParticipationDto,
    RouteParticipationStateDto, SearchQueryRequest, SearchResponseDto,
    SetRouteParticipationRequest, SocialContextDto, SuggestionResponseDto, TodayDto,
    UpdateActionRequest, UpdateContentRequest, UpdateJourneyRequest,
    UpdateKnowledgeResourceRequest, UpdateReminderPreferencesRequest, UserEventBatchRequest,
    UserEventIngestResponse, UserNotificationDto, WeeklyReviewDto,
};
use crate::{datasource::UpstreamError, domain::Domain};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) domain: Domain,
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::serve(
        "gateway",
        domain.config.listen_addr,
        router(AppState { domain }),
    )
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SuggestionQuery {
    q: String,
}

#[derive(Debug, Default, Deserialize)]
struct ScheduleQuery {
    date: Option<String>,
    timezone: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OwnContentQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    status: Option<bookway_api::ContentStatusDto>,
    strategy: Option<String>,
    content_type: Option<bookway_api::ContentTypeDto>,
    domain: Option<bookway_api::GrowthDomainDto>,
}

#[derive(Debug, Default, Deserialize)]
struct OwnAppealQuery {
    status: Option<bookway_api::ContentAppealStatusDto>,
    cursor: Option<String>,
    limit: Option<usize>,
}

pub(crate) fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(state.domain.config.cors_allowed_origins.clone())
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::ACCEPT,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::HeaderName::from_static("idempotency-key"),
            header::HeaderName::from_static("x-user-id"),
        ])
        .expose_headers([header::HeaderName::from_static("x-request-id")])
        .max_age(std::time::Duration::from_secs(600));
    Router::new()
        .route("/health", get(health))
        .route("/v1/journeys", get(list_journeys).post(create_journey))
        .route(
            "/v1/journeys/{journey_id}",
            get(get_journey).patch(update_journey),
        )
        .route("/v1/journeys/{journey_id}/actions", post(create_action))
        .route("/v1/today", get(today))
        .route("/v1/actions/{action_id}/complete", post(complete_action))
        .route("/v1/actions/{action_id}", patch(update_action))
        .route(
            "/v1/reminder-preferences",
            get(reminder_preferences).put(update_reminder_preferences),
        )
        .route("/v1/push-devices", post(register_push_device))
        .route("/v1/push-devices/{device_id}", delete(revoke_push_device))
        .route("/v1/notifications", get(list_notifications))
        .route(
            "/v1/notifications/{notification_id}/read",
            patch(mark_notification_read),
        )
        .route("/v1/entries", get(list_entries).post(create_entry))
        .route("/v1/reviews/weekly", get(weekly_review))
        .route("/v1/companion", get(companion))
        .route("/v1/knowledge", get(list_knowledge).post(create_knowledge))
        .route("/v1/knowledge/{resource_id}", patch(update_knowledge))
        .route("/v1/feed", get(feed))
        .route("/v1/search", get(search))
        .route("/v1/search/suggestions", get(suggestions))
        .route("/v1/events", post(ingest_events))
        .route("/v1/media/upload-url", post(create_media_upload))
        .route("/v1/media/{id}", get(get_media))
        .route("/v1/media/{id}/complete", post(complete_media_upload))
        .route("/v1/posts", post(create_content))
        .route("/v1/posts/{id}", get(get_content).patch(update_content))
        .route("/v1/posts/{id}/publish", post(publish_content))
        .route("/v1/posts/{id}/report", post(report_content))
        .route("/v1/posts/{id}/appeals", post(appeal_content))
        .route("/v1/me/posts", get(list_own_contents))
        .route("/v1/me/appeals", get(list_own_appeals))
        .route("/v1/moderation/reports", get(list_moderation_reports))
        .route(
            "/v1/moderation/reports/{report_id}",
            patch(review_moderation_report),
        )
        .route("/v1/moderation/appeals", get(list_moderation_appeals))
        .route(
            "/v1/moderation/appeals/{appeal_id}",
            patch(review_moderation_appeal),
        )
        .route("/v1/posts/{post_id}/reactions", put(set_reaction))
        .route(
            "/v1/posts/{post_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/v1/posts/{post_id}/comments/{comment_id}",
            delete(delete_comment),
        )
        .route("/v1/users/{user_id}/follow", put(set_follow))
        .route("/v1/social/context", get(social_context))
        .route("/v1/route-participations", get(list_route_participations))
        .route(
            "/v1/routes/{route_id}/participation",
            put(set_route_participation),
        )
        .route("/v1/routes/{route_id}/join", post(join_route))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "gateway".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn list_journeys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<JourneyDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.list_journeys(&user_id(&headers)).await?,
    )))
}

async fn create_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateJourneyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<JourneyDto>>), HttpError> {
    let journey = state
        .domain
        .create_journey(&user_id(&headers), request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(journey))))
}

async fn get_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(journey_id): Path<String>,
) -> Result<Json<ApiResponse<JourneyDetailDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .get_journey(&user_id(&headers), &journey_id)
            .await?,
    )))
}

async fn update_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(journey_id): Path<String>,
    Json(request): Json<UpdateJourneyRequest>,
) -> Result<Json<ApiResponse<JourneyDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_journey(&user_id(&headers), &journey_id, request)
            .await?,
    )))
}

async fn create_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(journey_id): Path<String>,
    Json(mut request): Json<CreateActionRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ActionDto>>), HttpError> {
    request.journey_id = journey_id;
    let action = state
        .domain
        .create_action(&user_id(&headers), request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(action))))
}

async fn today(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleQuery>,
) -> Result<Json<ApiResponse<TodayDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .today(
                &user_id(&headers),
                query.date.as_deref(),
                query.timezone.as_deref(),
            )
            .await?,
    )))
}

async fn complete_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(action_id): Path<String>,
) -> Result<Json<ApiResponse<ActionDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .complete_action(&user_id(&headers), &action_id)
            .await?,
    )))
}

async fn update_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(action_id): Path<String>,
    Json(request): Json<UpdateActionRequest>,
) -> Result<Json<ApiResponse<ActionDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_action(&user_id(&headers), &action_id, request)
            .await?,
    )))
}

async fn reminder_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ReminderPreferencesDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .reminder_preferences(&user_id(&headers))
            .await?,
    )))
}

async fn update_reminder_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateReminderPreferencesRequest>,
) -> Result<Json<ApiResponse<ReminderPreferencesDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_reminder_preferences(&user_id(&headers), request)
            .await?,
    )))
}

async fn register_push_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterPushDeviceRequest>,
) -> Result<(StatusCode, Json<ApiResponse<PushDeviceDto>>), HttpError> {
    let device = state
        .domain
        .register_push_device(&user_id(&headers), request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(device))))
}

async fn revoke_push_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    state
        .domain
        .revoke_push_device(&user_id(&headers), &device_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<NotificationQueryRequest>,
) -> Result<Json<ApiResponse<NotificationPageDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .list_notifications(&user_id(&headers), request)
            .await?,
    )))
}

async fn mark_notification_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(notification_id): Path<String>,
) -> Result<Json<ApiResponse<UserNotificationDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .mark_notification_read(&user_id(&headers), &notification_id)
            .await?,
    )))
}

async fn list_entries(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<GrowthEntryDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.list_entries(&user_id(&headers)).await?,
    )))
}

async fn create_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateGrowthEntryRequest>,
) -> Result<(StatusCode, Json<ApiResponse<GrowthEntryDto>>), HttpError> {
    let entry = state
        .domain
        .create_entry(&user_id(&headers), request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(entry))))
}

async fn weekly_review(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<WeeklyReviewDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.weekly_review(&user_id(&headers)).await?,
    )))
}

async fn companion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleQuery>,
) -> Result<Json<ApiResponse<CompanionBriefDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .companion(
                &user_id(&headers),
                query.date.as_deref(),
                query.timezone.as_deref(),
            )
            .await?,
    )))
}

async fn list_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<KnowledgeQueryRequest>,
) -> Result<Json<ApiResponse<Vec<KnowledgeResourceDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .list_knowledge(&user_id(&headers), request)
            .await?,
    )))
}

async fn create_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateKnowledgeResourceRequest>,
) -> Result<(StatusCode, Json<ApiResponse<KnowledgeResourceDto>>), HttpError> {
    let resource = state
        .domain
        .create_knowledge(&user_id(&headers), request, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(resource))))
}

async fn update_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Json(request): Json<UpdateKnowledgeResourceRequest>,
) -> Result<Json<ApiResponse<KnowledgeResourceDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_knowledge(&user_id(&headers), &resource_id, request)
            .await?,
    )))
}

async fn feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut request): Query<FeedQueryRequest>,
) -> Result<Json<ApiResponse<FeedDto>>, HttpError> {
    request.user_id = Some(user_id(&headers));
    Ok(Json(ApiResponse::new(state.domain.feed(request).await?)))
}

async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut request): Query<SearchQueryRequest>,
) -> Result<Json<ApiResponse<SearchResponseDto>>, HttpError> {
    request.user_id = Some(user_id(&headers));
    Ok(Json(ApiResponse::new(state.domain.search(request).await?)))
}

async fn suggestions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<SuggestionQuery>,
) -> Result<Json<ApiResponse<SuggestionResponseDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .suggestions(&user_id(&headers), &request.q)
            .await?,
    )))
}

async fn ingest_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UserEventBatchRequest>,
) -> Result<Json<ApiResponse<UserEventIngestResponse>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .ingest_events(&user_id(&headers), request)
            .await?,
    )))
}

async fn create_media_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MediaUploadRequest>,
) -> Result<(StatusCode, Json<ApiResponse<MediaUploadResponse>>), HttpError> {
    let media = state
        .domain
        .create_media_upload(&user_id(&headers), request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(media))))
}

async fn complete_media_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<MediaDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .complete_media_upload(&user_id(&headers), &id)
            .await?,
    )))
}

async fn get_media(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<MediaDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.get_media(&user_id(&headers), &id).await?,
    )))
}

async fn get_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.get_content(&user_id(&headers), &id).await?,
    )))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateContentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ContentDto>>), HttpError> {
    let content = state
        .domain
        .create_content(&user_id(&headers), request, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(content))))
}

async fn update_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<UpdateContentRequest>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_content(&user_id(&headers), &id, request)
            .await?,
    )))
}

async fn publish_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .publish_content(&user_id(&headers), &id)
            .await?,
    )))
}

async fn report_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CreateContentReportRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ContentReportDto>>), HttpError> {
    let report = state
        .domain
        .report_content(&user_id(&headers), &id, request, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(report))))
}

async fn appeal_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CreateContentAppealRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ContentAppealDto>>), HttpError> {
    let appeal = state
        .domain
        .appeal_content(&user_id(&headers), &id, request, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(appeal))))
}

async fn list_own_contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OwnContentQuery>,
) -> Result<Json<ApiResponse<ContentPageDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .own_contents(
                &user_id(&headers),
                ContentQueryRequest {
                    cursor: query.cursor,
                    limit: query.limit,
                    status: query.status,
                    strategy: query.strategy,
                    ids: None,
                    author_id: None,
                    content_type: query.content_type,
                    domain: query.domain,
                },
            )
            .await?,
    )))
}

async fn list_own_appeals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OwnAppealQuery>,
) -> Result<Json<ApiResponse<ContentAppealPageDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .own_appeals(
                &user_id(&headers),
                ContentAppealQueryRequest {
                    status: query.status,
                    appellant_id: None,
                    content_id: None,
                    cursor: query.cursor,
                    limit: query.limit,
                },
            )
            .await?,
    )))
}

async fn list_moderation_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<ContentReportQueryRequest>,
) -> Result<Json<ApiResponse<ContentReportPageDto>>, HttpError> {
    let _ = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.moderation_reports(request).await?,
    )))
}

async fn review_moderation_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(report_id): Path<String>,
    Json(request): Json<ReviewContentReportRequest>,
) -> Result<Json<ApiResponse<ContentReportDto>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state
            .domain
            .review_report(&reviewer_id, &report_id, request)
            .await?,
    )))
}

async fn list_moderation_appeals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<ContentAppealQueryRequest>,
) -> Result<Json<ApiResponse<ContentAppealPageDto>>, HttpError> {
    let _ = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.moderation_appeals(request).await?,
    )))
}

async fn review_moderation_appeal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(appeal_id): Path<String>,
    Json(request): Json<ReviewContentAppealRequest>,
) -> Result<Json<ApiResponse<ContentAppealDto>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state
            .domain
            .review_appeal(&reviewer_id, &appeal_id, request)
            .await?,
    )))
}

async fn set_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<ReactionRequest>,
) -> Result<Json<ApiResponse<ReactionDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .set_reaction(&user_id(&headers), &post_id, request)
            .await?,
    )))
}

async fn list_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Query(request): Query<CommentQueryRequest>,
) -> Result<Json<ApiResponse<CommentPageDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .comments(&user_id(&headers), &post_id, request)
            .await?,
    )))
}

async fn create_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CommentDto>>), HttpError> {
    let comment = state
        .domain
        .create_comment(
            &user_id(&headers),
            &post_id,
            request,
            idempotency_key(&headers),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(comment))))
}

async fn delete_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((post_id, comment_id)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    state
        .domain
        .delete_comment(&user_id(&headers), &post_id, &comment_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_follow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    Json(request): Json<FollowRequest>,
) -> Result<Json<ApiResponse<bookway_api::SocialContextDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .follow(&user_id(&headers), &target_user_id, request)
            .await?,
    )))
}

async fn social_context(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<SocialContextDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.social_context(&user_id(&headers)).await?,
    )))
}

async fn list_route_participations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<RouteParticipationDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .list_route_participations(&user_id(&headers))
            .await?,
    )))
}

async fn set_route_participation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(route_id): Path<String>,
    Json(request): Json<SetRouteParticipationRequest>,
) -> Result<Json<ApiResponse<RouteParticipationStateDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .set_route_participation(&user_id(&headers), &route_id, request)
            .await?,
    )))
}

async fn join_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(route_id): Path<String>,
) -> Result<Json<ApiResponse<RouteJoinResultDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .join_route(&user_id(&headers), &route_id)
            .await?,
    )))
}

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("demo-user")
        .to_string()
}

fn moderator_id(headers: &HeaderMap) -> Result<String, HttpError> {
    if !std::env::var("AUTH_REQUIRED").is_ok_and(|value| value == "true") {
        return Err(HttpError::Forbidden(
            "moderation endpoints require AUTH_REQUIRED=true".to_string(),
        ));
    }
    let authorized = headers
        .get("x-user-roles")
        .and_then(|value| value.to_str().ok())
        .is_some_and(has_moderator_role);
    if !authorized {
        return Err(HttpError::Forbidden(
            "moderator role is required".to_string(),
        ));
    }
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| HttpError::Forbidden("authenticated user is required".to_string()))
}

fn has_moderator_role(roles: &str) -> bool {
    roles
        .split(',')
        .map(str::trim)
        .any(|role| matches!(role, "moderator" | "admin" | "trust_safety"))
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

enum HttpError {
    Upstream(UpstreamError),
    Forbidden(String),
}

impl From<UpstreamError> for HttpError {
    fn from(value: UpstreamError) -> Self {
        Self::Upstream(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden(message) => error_response(StatusCode::FORBIDDEN, "forbidden", message),
            Self::Upstream(error) => match error {
                error @ UpstreamError::Transport { .. } => error_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream_unavailable",
                    error.to_string(),
                ),
                UpstreamError::Grpc {
                    service,
                    code,
                    message,
                } => {
                    let (status, error_code) = match code {
                        tonic::Code::InvalidArgument => {
                            (StatusCode::BAD_REQUEST, "invalid_argument")
                        }
                        tonic::Code::Unauthenticated => {
                            (StatusCode::UNAUTHORIZED, "unauthenticated")
                        }
                        tonic::Code::PermissionDenied => (StatusCode::FORBIDDEN, "forbidden"),
                        tonic::Code::NotFound => (StatusCode::NOT_FOUND, "not_found"),
                        tonic::Code::AlreadyExists | tonic::Code::Aborted => {
                            (StatusCode::CONFLICT, "conflict")
                        }
                        tonic::Code::ResourceExhausted => {
                            (StatusCode::TOO_MANY_REQUESTS, "rate_limited")
                        }
                        tonic::Code::FailedPrecondition => {
                            (StatusCode::UNPROCESSABLE_ENTITY, "failed_precondition")
                        }
                        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
                            (StatusCode::BAD_GATEWAY, "upstream_unavailable")
                        }
                        _ => (StatusCode::BAD_GATEWAY, "upstream_error"),
                    };
                    error_response(status, error_code, format!("{service}: {message}"))
                }
            },
        }
    }
}

fn error_response(status: StatusCode, code: &'static str, message: String) -> Response {
    (status, Json(ErrorResponse::new(code, message))).into_response()
}

#[cfg(test)]
mod tests {
    use super::has_moderator_role;

    #[test]
    fn moderator_roles_are_explicitly_allowlisted() {
        assert!(has_moderator_role("member, trust_safety"));
        assert!(has_moderator_role("admin"));
        assert!(!has_moderator_role("member, editor"));
        assert!(!has_moderator_role("supermoderator"));
    }
}
