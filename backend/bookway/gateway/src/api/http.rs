use crate::api::{ApiResponse, ErrorResponse, HealthResponse, rest};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use serde::Deserialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{datasource::UpstreamError, domain::Domain, domain::StockAccessError};
use bookway_account_api::pb as account_pb;
use bookway_ad_center_api::pb as ad_center_pb;
use bookway_ad_main_api::pb as ad_main_pb;
use bookway_bbs_api::pb as bbs_pb;
use bookway_bbs_creator_api::pb as creator_pb;
use bookway_bbs_link_api::pb as bbs_link_pb;
use bookway_bbs_message_api::pb as message_pb;
use bookway_comment_api::pb as comment_pb;
use bookway_content_audit_api::pb as audit_pb;
use bookway_feedback_api::pb as feedback_pb;
use bookway_growth_api::pb as growth_pb;
use bookway_knowledge_catalog_api::pb as catalog_pb;
use bookway_mall_api::pb as mall_pb;
use bookway_mall_inventory_api::pb as mall_inventory_pb;
use bookway_mall_order_api::pb as mall_order_pb;
use bookway_media_api::pb as media_pb;
use bookway_user_event_api::pb as user_event_pb;

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
    limit: Option<u32>,
    status: Option<rest::ContentStatus>,
    strategy: Option<String>,
    content_type: Option<rest::ContentType>,
    domain: Option<rest::GrowthDomain>,
}

#[derive(Debug, Default, Deserialize)]
struct PublicAuthorContentQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct AdQuery {
    placement: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
    domain: Option<String>,
    limit: Option<u32>,
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
            header::HeaderName::from_static("x-user-roles"),
        ])
        .expose_headers([header::HeaderName::from_static("x-request-id")])
        .max_age(std::time::Duration::from_secs(600));
    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/me/profile",
            get(account_profile).patch(update_account_profile),
        )
        .route(
            "/v1/me/creator-profile",
            get(own_creator_profile).put(update_creator_profile),
        )
        .route("/v1/creators", get(list_creator_profiles))
        .route("/v1/creators/{user_id}", get(get_creator_profile))
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
        .route("/v1/messages", post(send_direct_message))
        .route(
            "/v1/messages/{message_id}/report",
            post(report_direct_message),
        )
        .route("/v1/messages/conversations", get(list_direct_conversations))
        .route(
            "/v1/messages/conversations/{conversation_id}",
            get(list_direct_messages),
        )
        .route(
            "/v1/messages/conversations/{conversation_id}/read",
            post(mark_direct_conversation_read),
        )
        .route(
            "/v1/message-preferences",
            get(get_direct_message_preferences).put(update_direct_message_preferences),
        )
        .route("/v1/entries", get(list_entries).post(create_entry))
        .route(
            "/v1/entries/{entry_id}/publication/retry",
            post(retry_entry_publication),
        )
        .route(
            "/v1/reviews/weekly",
            get(weekly_review).put(save_weekly_review),
        )
        .route(
            "/v1/reviews/{review_id}/adjustments/{suggestion_index}/apply",
            post(apply_weekly_review_adjustment),
        )
        .route("/v1/companion", get(companion))
        .route("/v1/knowledge", get(list_knowledge).post(create_knowledge))
        .route(
            "/v1/knowledge/{resource_id}/journey",
            post(start_knowledge_journey),
        )
        .route("/v1/knowledge/{resource_id}", patch(update_knowledge))
        .route("/v1/feed", get(feed))
        .route("/v1/ads", get(ad_decisions))
        .route("/v1/ads/events", post(report_ad_event))
        .route(
            "/v1/admin/ads/campaigns",
            get(admin_ad_campaigns).post(admin_create_ad_campaign),
        )
        .route(
            "/v1/admin/ads/campaigns/{campaign_id}",
            get(admin_ad_campaign).patch(admin_update_ad_campaign),
        )
        .route(
            "/v1/admin/ads/guardrails",
            get(admin_ad_guardrails).patch(admin_set_ad_guardrails),
        )
        .route("/v1/admin/ads/reports", get(admin_ad_delivery_report))
        .route(
            "/v1/admin/mall/products",
            get(admin_mall_products).post(admin_create_mall_product),
        )
        .route(
            "/v1/admin/mall/products/{product_id}",
            patch(admin_update_mall_product),
        )
        .route(
            "/v1/admin/mall/skus/{sku_id}/stock",
            post(admin_set_mall_sku_stock),
        )
        .route(
            "/v1/admin/routes/{route_id}/nodes/{action_node_id}/offers",
            post(admin_attach_mall_node_offer),
        )
        .route("/v1/admin/mall/orders", get(admin_mall_orders))
        .route(
            "/v1/admin/mall/orders/{order_id}/fulfillment",
            post(admin_update_mall_fulfillment),
        )
        .route(
            "/v1/admin/mall/affiliate-settlements",
            get(admin_affiliate_settlements),
        )
        .route(
            "/v1/admin/mall/affiliate-settlements/{settlement_id}/settle",
            post(admin_settle_affiliate),
        )
        .route(
            "/v1/routes/{route_id}/nodes/{action_node_id}/offers",
            get(route_node_offers),
        )
        .route(
            "/v1/routes/{route_id}/nodes/{action_node_id}/resources",
            get(list_route_node_resources).post(attach_route_node_resource),
        )
        .route(
            "/v1/routes/{route_id}/nodes/{action_node_id}/resources/{attachment_id}",
            delete(detach_route_node_resource),
        )
        .route(
            "/v1/routes/{route_id}/nodes/{action_node_id}/rag-context",
            post(route_node_rag_context),
        )
        .route("/v1/orders", get(mall_orders).post(create_mall_order))
        .route("/v1/orders/{order_id}", get(mall_order))
        .route("/v1/orders/{order_id}/cancel", post(cancel_mall_order))
        .route("/v1/search", get(search))
        .route("/v1/search/suggestions", get(suggestions))
        .route("/v1/resources", get(search_resources))
        .route("/v1/resources/{resource_id}", get(get_resource))
        .route(
            "/v1/resources/{resource_id}/knowledge",
            post(capture_resource_knowledge),
        )
        .route("/v1/events", post(ingest_events))
        .route("/v1/feedback", post(create_feedback))
        .route("/v1/me/feedback", get(list_own_feedback))
        .route("/v1/moderation/feedback", get(list_moderation_feedback))
        .route(
            "/v1/moderation/feedback/{feedback_id}",
            patch(review_moderation_feedback),
        )
        .route("/v1/media/upload-url", post(create_media_upload))
        .route("/v1/media/{id}", get(get_media))
        .route("/v1/media/{id}/complete", post(complete_media_upload))
        .route("/v1/posts", post(create_content))
        .route("/v1/posts/{id}", get(get_content).patch(update_content))
        .route("/v1/posts/{id}/fork", post(fork_route))
        .route("/v1/posts/{id}/publish", post(publish_content))
        .route(
            "/v1/posts/{id}/knowledge",
            post(capture_content_as_knowledge),
        )
        .route("/v1/posts/{id}/report", post(report_content))
        .route("/v1/posts/{id}/appeals", post(appeal_content))
        .route("/v1/me/posts", get(list_own_contents))
        .route(
            "/v1/users/{user_id}/posts",
            get(list_public_author_contents),
        )
        .route("/v1/me/appeals", get(list_own_appeals))
        .route("/v1/moderation/reports", get(list_moderation_reports))
        .route(
            "/v1/moderation/reports/{report_id}",
            patch(review_moderation_report),
        )
        .route(
            "/v1/moderation/message-reports",
            get(list_moderation_direct_message_reports),
        )
        .route(
            "/v1/moderation/message-reports/{report_id}",
            patch(review_moderation_direct_message_report),
        )
        .route(
            "/v1/moderation/comment-reports",
            get(list_moderation_comment_reports),
        )
        .route(
            "/v1/moderation/comment-reports/{report_id}",
            patch(review_moderation_comment_report),
        )
        .route(
            "/v1/moderation/comment-appeals",
            get(list_moderation_comment_appeals),
        )
        .route(
            "/v1/moderation/comment-appeals/{appeal_id}",
            patch(review_moderation_comment_appeal),
        )
        .route("/v1/moderation/appeals", get(list_moderation_appeals))
        .route(
            "/v1/moderation/appeals/{appeal_id}",
            patch(review_moderation_appeal),
        )
        .route("/v1/moderation/comments", get(list_moderation_comments))
        .route(
            "/v1/moderation/comments/{comment_id}",
            patch(review_moderation_comment),
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
        .route(
            "/v1/posts/{post_id}/comments/{comment_id}/accept",
            post(accept_question_answer),
        )
        .route(
            "/v1/posts/{post_id}/comments/{comment_id}/report",
            post(report_comment),
        )
        .route("/v1/comments/{comment_id}/appeals", post(appeal_comment))
        .route("/v1/me/comment-appeals", get(list_own_comment_appeals))
        .route("/v1/users/{user_id}/follow", put(set_follow))
        .route("/v1/users/{user_id}/relationship", put(set_relationship))
        .route("/v1/users/{user_id}/followers", get(list_followers))
        .route("/v1/users/{user_id}/social-stats", get(social_stats))
        .route("/v1/social/context", get(social_context))
        .route("/v1/route-participations", get(list_route_participations))
        .route("/v1/routes/{route_id}/participation", put(set_route_participation))
        .route("/v1/routes/{route_id}/peers", get(list_route_peers))
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

async fn account_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<account_pb::AccountProfile>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .account_profile(account_pb::ProfileRequest {
                user_id: user_id(&headers),
            })
            .await?,
    )))
}

async fn update_account_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::UpdateAccountProfileRequest>,
) -> Result<Json<ApiResponse<account_pb::AccountProfile>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_account_profile(request.into_pb(user_id(&headers)))
            .await?,
    )))
}

async fn own_creator_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<rest::CreatorProfile>>, HttpError> {
    let profile = state
        .domain
        .creator_profile(creator_pb::CreatorProfileRequest {
            user_id: user_id(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(
        profile.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn update_creator_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::UpdateCreatorProfileRequest>,
) -> Result<Json<ApiResponse<rest::CreatorProfile>>, HttpError> {
    let profile = state
        .domain
        .update_creator_profile(request.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(
        profile.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_creator_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::CreatorProfileQuery>,
) -> Result<Json<ApiResponse<rest::CreatorProfilePage>>, HttpError> {
    let profiles = state
        .domain
        .public_creator_profiles(&user_id(&headers), query.into_pb())
        .await?;
    Ok(Json(ApiResponse::new(
        profiles.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn get_creator_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(creator_user_id): Path<String>,
) -> Result<Json<ApiResponse<rest::CreatorProfile>>, HttpError> {
    let profile = state
        .domain
        .public_creator_profile(
            &user_id(&headers),
            creator_pb::CreatorProfileRequest {
                user_id: creator_user_id,
            },
        )
        .await?;
    Ok(Json(ApiResponse::new(
        profile.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn send_direct_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::SendDirectMessageRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::DirectMessage>>), HttpError> {
    let request = request
        .into_pb(user_id(&headers), idempotency_key(&headers))
        .map_err(HttpError::InvalidRequest)?;
    let message = state.domain.send_direct_message(request).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            message.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn report_direct_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Json(request): Json<rest::CreateDirectMessageReportRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rest::DirectMessageReportReceipt>>,
    ),
    HttpError,
> {
    let report = state
        .domain
        .report_direct_message(
            request
                .into_pb(user_id(&headers), message_id, idempotency_key(&headers))
                .map_err(HttpError::InvalidRequest)?,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            report.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn list_direct_conversations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::DirectConversationQuery>,
) -> Result<Json<ApiResponse<rest::DirectConversationPage>>, HttpError> {
    let page = state
        .domain
        .direct_conversations(query.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(page.into())))
}

async fn list_direct_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    Query(query): Query<rest::DirectMessageQuery>,
) -> Result<Json<ApiResponse<rest::DirectMessagePage>>, HttpError> {
    let page = state
        .domain
        .direct_messages(query.into_pb(user_id(&headers), conversation_id))
        .await?;
    Ok(Json(ApiResponse::new(
        page.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn mark_direct_conversation_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    request: Option<Json<rest::MarkConversationReadRequest>>,
) -> Result<Json<ApiResponse<rest::MarkConversationReadResponse>>, HttpError> {
    let request = request
        .map(|request| request.0)
        .unwrap_or_default()
        .into_pb(user_id(&headers), conversation_id);
    let response = state.domain.mark_direct_conversation_read(request).await?;
    Ok(Json(ApiResponse::new(response.into())))
}

async fn get_direct_message_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<rest::DirectMessagePreferences>>, HttpError> {
    let preferences = state
        .domain
        .direct_message_preferences(message_pb::UserRequest {
            user_id: user_id(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(preferences.into())))
}

async fn update_direct_message_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::UpdateDirectMessagePreferencesRequest>,
) -> Result<Json<ApiResponse<rest::DirectMessagePreferences>>, HttpError> {
    let preferences = state
        .domain
        .update_direct_message_preferences(request.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(preferences.into())))
}

async fn list_journeys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<rest::JourneyList>>, HttpError> {
    let journeys = state
        .domain
        .list_journeys(growth_pb::UserRequest {
            user_id: user_id(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(
        journeys.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn create_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::CreateJourneyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::Journey>>), HttpError> {
    let request = request
        .into_pb(user_id(&headers), idempotency_key(&headers))
        .map_err(HttpError::InvalidRequest)?;
    let journey = state.domain.create_journey(request).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            journey.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn get_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(journey_id): Path<String>,
) -> Result<Json<ApiResponse<rest::JourneyDetail>>, HttpError> {
    let detail = state
        .domain
        .get_journey(growth_pb::JourneyRequest {
            user_id: user_id(&headers),
            journey_id,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        detail.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn update_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(journey_id): Path<String>,
    Json(request): Json<rest::UpdateJourneyRequest>,
) -> Result<Json<ApiResponse<rest::Journey>>, HttpError> {
    let journey = state
        .domain
        .update_journey(request.into_pb(user_id(&headers), journey_id))
        .await?;
    Ok(Json(ApiResponse::new(
        journey.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn create_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(journey_id): Path<String>,
    Json(request): Json<rest::CreateActionRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::Action>>), HttpError> {
    let request = request
        .into_pb(user_id(&headers), journey_id, idempotency_key(&headers))
        .map_err(HttpError::InvalidRequest)?;
    let action = state.domain.create_action(request).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            action.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn today(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleQuery>,
) -> Result<Json<ApiResponse<rest::TodaySummary>>, HttpError> {
    let today = state
        .domain
        .today(growth_pb::ScheduleRequest {
            user_id: user_id(&headers),
            local_date: query.date,
            timezone: query.timezone,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        today.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn complete_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(action_id): Path<String>,
) -> Result<Json<ApiResponse<rest::Action>>, HttpError> {
    let action = state
        .domain
        .complete_action(growth_pb::CompleteActionRequest {
            user_id: user_id(&headers),
            action_id,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        action.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn update_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(action_id): Path<String>,
    Json(request): Json<rest::UpdateActionRequest>,
) -> Result<Json<ApiResponse<rest::Action>>, HttpError> {
    let action = state
        .domain
        .update_action(request.into_pb(user_id(&headers), action_id))
        .await?;
    Ok(Json(ApiResponse::new(
        action.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn reminder_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<growth_pb::ReminderPreference>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .reminder_preferences(growth_pb::UserRequest {
                user_id: user_id(&headers),
            })
            .await?,
    )))
}

async fn update_reminder_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::UpdateReminderPreferencesRequest>,
) -> Result<Json<ApiResponse<growth_pb::ReminderPreference>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .update_reminder_preferences(request.into_pb(user_id(&headers)))
            .await?,
    )))
}

async fn register_push_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<growth_pb::RegisterPushDeviceRequest>,
) -> Result<(StatusCode, Json<ApiResponse<growth_pb::PushDevice>>), HttpError> {
    request.user_id = user_id(&headers);
    let device = state.domain.register_push_device(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(device))))
}

async fn revoke_push_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    state
        .domain
        .revoke_push_device(growth_pb::PushDeviceRequest {
            user_id: user_id(&headers),
            device_id,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::NotificationQuery>,
) -> Result<Json<ApiResponse<rest::NotificationPage>>, HttpError> {
    let notifications = state
        .domain
        .list_notifications(query.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(
        notifications.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn mark_notification_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(notification_id): Path<String>,
) -> Result<Json<ApiResponse<rest::UserNotification>>, HttpError> {
    let notification = state
        .domain
        .mark_notification_read(growth_pb::NotificationRequest {
            user_id: user_id(&headers),
            notification_id,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        notification.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_entries(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<rest::EntryList>>, HttpError> {
    let entries = state
        .domain
        .list_entries(growth_pb::UserRequest {
            user_id: user_id(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(
        entries.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn create_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::CreateEntryRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::GrowthEntry>>), HttpError> {
    let entry = state
        .domain
        .create_entry(request.into_pb(user_id(&headers), idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            entry.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn retry_entry_publication(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
) -> Result<Json<ApiResponse<rest::GrowthEntry>>, HttpError> {
    let entry = state
        .domain
        .retry_entry_publication(growth_pb::RetryEntryPublicationRequest {
            user_id: user_id(&headers),
            entry_id,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        entry.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn weekly_review(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<rest::WeeklyReviewSummary>>, HttpError> {
    let review = state
        .domain
        .weekly_review(growth_pb::UserRequest {
            user_id: user_id(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(
        review.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn save_weekly_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::SaveWeeklyReviewRequest>,
) -> Result<Json<ApiResponse<rest::WeeklyReview>>, HttpError> {
    let review = state
        .domain
        .save_weekly_review(request.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(
        review.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn apply_weekly_review_adjustment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((review_id, suggestion_index)): Path<(String, u32)>,
) -> Result<Json<ApiResponse<rest::ReviewAdjustmentApplication>>, HttpError> {
    let applied = state
        .domain
        .apply_weekly_review_adjustment(growth_pb::ApplyWeeklyReviewAdjustmentRequest {
            user_id: user_id(&headers),
            review_id,
            suggestion_index,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        applied.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn companion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleQuery>,
) -> Result<Json<ApiResponse<rest::CompanionBrief>>, HttpError> {
    let companion = state
        .domain
        .companion(growth_pb::ScheduleRequest {
            user_id: user_id(&headers),
            local_date: query.date,
            timezone: query.timezone,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        companion.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::KnowledgeQuery>,
) -> Result<Json<ApiResponse<rest::KnowledgeList>>, HttpError> {
    let resources = state
        .domain
        .list_knowledge(query.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(
        resources.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn create_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::CreateKnowledgeRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::KnowledgeResource>>), HttpError> {
    let resource = state
        .domain
        .create_knowledge(request.into_pb(user_id(&headers), idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            resource.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn update_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Json(request): Json<rest::UpdateKnowledgeRequest>,
) -> Result<Json<ApiResponse<rest::KnowledgeResource>>, HttpError> {
    let resource = state
        .domain
        .update_knowledge(request.into_pb(user_id(&headers), resource_id))
        .await?;
    Ok(Json(ApiResponse::new(
        resource.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn start_knowledge_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Json(request): Json<rest::StartKnowledgeJourneyRequest>,
) -> Result<Json<ApiResponse<rest::KnowledgeJourney>>, HttpError> {
    let journey = state
        .domain
        .start_knowledge_journey(
            request
                .into_pb(user_id(&headers), resource_id)
                .map_err(HttpError::InvalidRequest)?,
        )
        .await?;
    Ok(Json(ApiResponse::new(
        journey.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::FeedQuery>,
) -> Result<Json<ApiResponse<rest::FeedResponse>>, HttpError> {
    let request = query
        .into_pb(user_id(&headers))
        .map_err(HttpError::InvalidRequest)?;
    let response = state.domain.feed(request).await?;
    Ok(Json(ApiResponse::new(
        response.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn ad_decisions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdQuery>,
) -> Result<Json<ApiResponse<ad_main_pb::DecisionResponse>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .ad_decisions(ad_main_pb::DecisionRequest {
                user_id: user_id(&headers),
                placement: query.placement,
                domain: query.domain,
                limit: query.limit,
                route_id: query.route_id,
                action_node_id: query.action_node_id,
                scene_equipment: Some(query.scene_equipment),
                // The client's user agent is the one delivery dimension a
                // browser reliably exposes today; region stays empty until a
                // trustworthy source exists, which keeps geo-targeted stock
                // out of these requests by design (fail-closed).
                geo_region: String::new(),
                device_os: device_os_from_user_agent(&headers),
            })
            .await?,
    )))
}

/// Classifies the request user agent into the documented targeting slug
/// ("ios", "android", "web"); an unreadable agent means unknown, so the
/// decision can only match unrestricted campaigns.
fn device_os_from_user_agent(headers: &HeaderMap) -> String {
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_lowercase();
    if agent.is_empty() {
        return String::new();
    }
    if agent.contains("iphone") || agent.contains("ipad") || agent.contains("ios") {
        "ios".to_string()
    } else if agent.contains("android") {
        "android".to_string()
    } else {
        "web".to_string()
    }
}

async fn report_ad_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<ad_center_pb::RecordEventRequest>,
) -> Result<Json<ApiResponse<ad_center_pb::EventReceipt>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .report_ad_event({
                request.user_id = user_id(&headers);
                request
            })
            .await?,
    )))
}

async fn admin_ad_campaigns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut request): Query<ad_center_pb::AdvertiserCampaignQuery>,
) -> Result<Json<ApiResponse<ad_center_pb::CampaignList>>, HttpError> {
    request.advertiser_id = advertiser_admin_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.advertiser_campaigns(request).await?,
    )))
}

async fn admin_ad_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign_id): Path<String>,
) -> Result<Json<ApiResponse<ad_center_pb::AdCampaign>>, HttpError> {
    let campaign = state
        .domain
        .advertiser_campaign(ad_center_pb::CampaignIdRequest {
            campaign_id,
            advertiser_id: advertiser_admin_id(&headers)?,
        })
        .await?;
    Ok(Json(ApiResponse::new(campaign)))
}

async fn admin_create_ad_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<ad_center_pb::CreateCampaignRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ad_center_pb::AdCampaign>>), HttpError> {
    request.advertiser_id = advertiser_admin_id(&headers)?;
    let campaign = state.domain.create_advertiser_campaign(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(campaign))))
}

async fn admin_update_ad_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign_id): Path<String>,
    Json(mut request): Json<ad_center_pb::UpdateCampaignRequest>,
) -> Result<Json<ApiResponse<ad_center_pb::AdCampaign>>, HttpError> {
    request.advertiser_id = advertiser_admin_id(&headers)?;
    request.campaign_id = campaign_id;
    Ok(Json(ApiResponse::new(
        state.domain.update_advertiser_campaign(request).await?,
    )))
}

async fn admin_ad_guardrails(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ad_center_pb::DeliveryGuardrails>>, HttpError> {
    // Advertisers may observe the cap that bounds their delivery; only the
    // platform can change it.
    advertiser_admin_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.ad_delivery_guardrails().await?,
    )))
}

async fn admin_set_ad_guardrails(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ad_center_pb::DeliveryGuardrails>,
) -> Result<Json<ApiResponse<ad_center_pb::DeliveryGuardrails>>, HttpError> {
    // A cap an advertiser could loosen would not be a guardrail.
    platform_admin_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.set_ad_user_daily_total_cap(request).await?,
    )))
}

async fn admin_ad_delivery_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut request): Query<ad_center_pb::AdDeliveryReportRequest>,
) -> Result<Json<ApiResponse<ad_center_pb::AdDeliveryReport>>, HttpError> {
    request.advertiser_id = advertiser_admin_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.advertiser_delivery_report(request).await?,
    )))
}

async fn admin_mall_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut request): Query<mall_pb::ProductQueryRequest>,
) -> Result<Json<ApiResponse<mall_pb::ProductPage>>, HttpError> {
    request.merchant_id = Some(merchant_admin_id(&headers)?);
    request.include_inactive = true;
    Ok(Json(ApiResponse::new(
        state.domain.mall_products(request).await?,
    )))
}

async fn admin_create_mall_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<mall_pb::CreateProductRequest>,
) -> Result<(StatusCode, Json<ApiResponse<mall_pb::MallProduct>>), HttpError> {
    request.merchant_id = merchant_admin_id(&headers)?;
    let product = state.domain.create_mall_product(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(product))))
}

async fn admin_update_mall_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product_id): Path<String>,
    Json(mut request): Json<mall_pb::UpdateProductRequest>,
) -> Result<Json<ApiResponse<mall_pb::MallProduct>>, HttpError> {
    request.merchant_id = merchant_admin_id(&headers)?;
    request.product_id = product_id;
    Ok(Json(ApiResponse::new(
        state.domain.update_mall_product(request).await?,
    )))
}

#[derive(Deserialize)]
struct SetMallSkuStockBody {
    available: i64,
}

/// Merchant stock adjustment. The gateway proves SKU ownership through mall
/// before mall-inventory applies the absolute count; inventory still rejects
/// reductions below the reserved quantity.
async fn admin_set_mall_sku_stock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sku_id): Path<String>,
    Json(body): Json<SetMallSkuStockBody>,
) -> Result<Json<ApiResponse<mall_inventory_pb::InventoryItem>>, HttpError> {
    let merchant_id = merchant_admin_id(&headers)?;
    match state
        .domain
        .set_mall_sku_stock(merchant_id, sku_id, body.available)
        .await
    {
        Ok(item) => Ok(Json(ApiResponse::new(item))),
        Err(StockAccessError::Forbidden) => Err(HttpError::Forbidden(
            "sku does not belong to this merchant".to_string(),
        )),
        Err(StockAccessError::Upstream(upstream)) => Err(HttpError::from(upstream)),
    }
}

async fn admin_attach_mall_node_offer(    State(state): State<AppState>,
    headers: HeaderMap,
    Path((route_id, action_node_id)): Path<(String, String)>,
    Json(mut request): Json<mall_pb::AttachNodeOfferRequest>,
) -> Result<(StatusCode, Json<ApiResponse<mall_pb::NodeOffer>>), HttpError> {
    request.merchant_id = merchant_admin_id(&headers)?;
    request.route_id = route_id;
    request.action_node_id = action_node_id;
    request.idempotency_key = idempotency_key(&headers).unwrap_or_default();
    let offer = state.domain.attach_mall_node_offer(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(offer))))
}

async fn route_node_offers(
    State(state): State<AppState>,
    Path((route_id, action_node_id)): Path<(String, String)>,
    Query(query): Query<mall_pb::NodeOfferQueryRequest>,
) -> Result<Json<ApiResponse<mall_pb::NodeOfferList>>, HttpError> {
    let request = mall_pb::NodeOfferQueryRequest {
        route_id,
        action_node_id,
        limit: query.limit,
        scene_equipment: query.scene_equipment,
    };
    Ok(Json(ApiResponse::new(
        state.domain.mall_node_offers(request).await?,
    )))
}

async fn admin_mall_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut query): Query<mall_order_pb::MerchantOrderRequest>,
) -> Result<Json<ApiResponse<mall_order_pb::MerchantOrderListResponse>>, HttpError> {
    query.merchant_id = merchant_admin_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.merchant_mall_orders(query).await?,
    )))
}

async fn admin_update_mall_fulfillment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(mut request): Json<mall_order_pb::UpdateFulfillmentRequest>,
) -> Result<Json<ApiResponse<mall_order_pb::Order>>, HttpError> {
    request.merchant_id = merchant_admin_id(&headers)?;
    request.order_id = order_id;
    Ok(Json(ApiResponse::new(
        state.domain.update_mall_fulfillment(request).await?,
    )))
}

async fn admin_affiliate_settlements(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut query): Query<mall_order_pb::AffiliateSettlementRequest>,
) -> Result<Json<ApiResponse<mall_order_pb::AffiliateSettlementListResponse>>, HttpError> {
    query.merchant_id = merchant_admin_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.affiliate_settlements(query).await?,
    )))
}

async fn admin_settle_affiliate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(settlement_id): Path<String>,
) -> Result<Json<ApiResponse<mall_order_pb::AffiliateSettlement>>, HttpError> {
    let request = mall_order_pb::SettleAffiliateRequest {
        merchant_id: merchant_admin_id(&headers)?,
        settlement_id,
    };
    Ok(Json(ApiResponse::new(
        state.domain.settle_affiliate(request).await?,
    )))
}

async fn create_mall_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<mall_order_pb::CreateRequest>,
) -> Result<(StatusCode, Json<ApiResponse<mall_order_pb::Order>>), HttpError> {
    request.user_id = user_id(&headers);
    request.idempotency_key = idempotency_key(&headers).unwrap_or_default();
    let order = state.domain.create_mall_order(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(order))))
}

async fn mall_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<mall_order_pb::OrderListResponse>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .mall_orders(mall_order_pb::UserRequest {
                user_id: user_id(&headers),
            })
            .await?,
    )))
}

async fn mall_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<ApiResponse<mall_order_pb::Order>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .mall_order(mall_order_pb::OrderRequest {
                user_id: user_id(&headers),
                order_id,
            })
            .await?,
    )))
}

async fn cancel_mall_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<ApiResponse<mall_order_pb::Order>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .cancel_mall_order(mall_order_pb::OrderRequest {
                user_id: user_id(&headers),
                order_id,
            })
            .await?,
    )))
}

async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::SearchQuery>,
) -> Result<Json<ApiResponse<rest::SearchResponse>>, HttpError> {
    let response = state
        .domain
        .search(query.into_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(
        response.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn suggestions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<SuggestionQuery>,
) -> Result<Json<ApiResponse<rest::SuggestionsResponse>>, HttpError> {
    let response = state
        .domain
        .suggestions(user_id(&headers), request.q)
        .await?;
    Ok(Json(ApiResponse::new(
        response.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn search_resources(
    State(state): State<AppState>,
    Query(query): Query<rest::ResourceSearchQuery>,
) -> Result<Json<ApiResponse<rest::PublicResourcePage>>, HttpError> {
    let response = state
        .domain
        .search_resources(query.into_pb().map_err(HttpError::InvalidRequest)?)
        .await?;
    Ok(Json(ApiResponse::new(
        response.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_route_node_resources(
    State(state): State<AppState>,
    Path((route_id, action_node_id)): Path<(String, String)>,
    Query(query): Query<rest::RouteNodeResourceQuery>,
) -> Result<Json<ApiResponse<rest::RouteNodeResourcePage>>, HttpError> {
    // This is a public customer-facing route. Archived attachments are an
    // author/admin maintenance view and must not be exposed through the
    // Gateway without a separate authenticated management endpoint.
    if query.include_archived {
        return Err(HttpError::Forbidden(
            "archived route resources are not publicly readable".to_string(),
        ));
    }
    let response = state
        .domain
        .list_route_node_resources(catalog_pb::ListNodeResourcesRequest {
            route_id,
            action_node_id,
            include_archived: false,
            scene_equipment: query.scene_equipment,
        })
        .await?;
    Ok(Json(ApiResponse::new(
        response.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn attach_route_node_resource(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((route_id, action_node_id)): Path<(String, String)>,
    Json(request): Json<rest::AttachRouteNodeResourceRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rest::RouteNodeResourceAttachment>>,
    ),
    HttpError,
> {
    let request = request
        .into_pb(
            route_id,
            action_node_id,
            user_id(&headers),
            idempotency_key(&headers),
        )
        .map_err(HttpError::InvalidRequest)?;
    let attachment = state.domain.attach_route_node_resource(request).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            attachment.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn detach_route_node_resource(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((route_id, action_node_id, attachment_id)): Path<(String, String, String)>,
) -> Result<Json<ApiResponse<rest::DetachRouteNodeResourceResponse>>, HttpError> {
    let response = state
        .domain
        .detach_route_node_resource(catalog_pb::DetachNodeResourceRequest {
            route_id,
            action_node_id,
            attachment_id,
            operator_id: user_id(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(response.into())))
}

async fn route_node_rag_context(
    State(state): State<AppState>,
    Path((route_id, action_node_id)): Path<(String, String)>,
    Json(request): Json<rest::RouteNodeRagContextRequest>,
) -> Result<Json<ApiResponse<rest::RouteNodeRagContextResponse>>, HttpError> {
    let response = state
        .domain
        .retrieve_route_node_rag_context(request.into_pb(route_id, action_node_id))
        .await?;
    Ok(Json(ApiResponse::new(
        response.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn get_resource(
    State(state): State<AppState>,
    Path(resource_id): Path<String>,
) -> Result<Json<ApiResponse<rest::PublicResource>>, HttpError> {
    let resource = state.domain.get_resource(resource_id).await?;
    Ok(Json(ApiResponse::new(
        resource.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn capture_resource_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
) -> Result<(StatusCode, Json<ApiResponse<growth_pb::KnowledgeResource>>), HttpError> {
    let resource = state
        .domain
        .capture_resource_as_knowledge(user_id(&headers), resource_id)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(resource))))
}

async fn ingest_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::IngestEventsRequest>,
) -> Result<Json<ApiResponse<user_event_pb::IngestResponse>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .ingest_events(
                request
                    .into_pb(user_id(&headers))
                    .map_err(HttpError::InvalidRequest)?,
            )
            .await?,
    )))
}

async fn create_feedback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::CreateFeedbackRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::FeedbackItem>>), HttpError> {
    let feedback = state
        .domain
        .create_feedback(request.into_pb(user_id(&headers), idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            feedback.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn list_own_feedback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::FeedbackQuery>,
) -> Result<Json<ApiResponse<rest::FeedbackList>>, HttpError> {
    let feedback = state
        .domain
        .own_feedback(query.into_own_pb(user_id(&headers)))
        .await?;
    Ok(Json(ApiResponse::new(
        feedback.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_moderation_feedback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<feedback_pb::ListFeedbackRequest>,
) -> Result<Json<ApiResponse<feedback_pb::FeedbackList>>, HttpError> {
    let _ = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.moderation_feedback(query).await?,
    )))
}

async fn review_moderation_feedback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(feedback_id): Path<String>,
    Json(mut request): Json<feedback_pb::ReviewFeedbackRequest>,
) -> Result<Json<ApiResponse<feedback_pb::FeedbackItem>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state
            .domain
            .review_feedback({
                request.reviewer_id = reviewer_id;
                request.feedback_id = feedback_id;
                request
            })
            .await?,
    )))
}

async fn create_media_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::CreateMediaUploadRequest>,
) -> Result<(StatusCode, Json<ApiResponse<media_pb::UploadResponse>>), HttpError> {
    let media = state
        .domain
        .create_media_upload(request.into_pb(user_id(&headers)))
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(media))))
}

async fn complete_media_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<media_pb::MediaResource>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .complete_media_upload(media_pb::ResourceRequest {
                user_id: user_id(&headers),
                id,
            })
            .await?,
    )))
}

async fn get_media(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<media_pb::MediaResource>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .get_media(media_pb::ResourceRequest {
                user_id: user_id(&headers),
                id,
            })
            .await?,
    )))
}

async fn get_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<rest::Content>>, HttpError> {
    let content = state.domain.get_content(&user_id(&headers), &id).await?;
    Ok(Json(ApiResponse::new(
        content.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<rest::CreateContentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::Content>>), HttpError> {
    let content = state
        .domain
        .create_content(request.into_pb(user_id(&headers), idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            content.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn fork_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(source_route_id): Path<String>,
    Json(request): Json<rest::ForkRouteRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::Content>>), HttpError> {
    let request = request.into_pb(
        user_id(&headers),
        source_route_id,
        required_idempotency_key(&headers)?,
    );
    let content = state.domain.fork_route(request).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            content.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn update_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<rest::UpdateContentRequest>,
) -> Result<Json<ApiResponse<rest::Content>>, HttpError> {
    let content = state
        .domain
        .update_content(request.into_pb(user_id(&headers), id))
        .await?;
    Ok(Json(ApiResponse::new(
        content.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn publish_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<rest::Content>>, HttpError> {
    let content = state
        .domain
        .publish_content(bbs_link_pb::PublishRequest {
            user_id: user_id(&headers),
            id,
            idempotency_key: idempotency_key(&headers),
        })
        .await?;
    Ok(Json(ApiResponse::new(
        content.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn capture_content_as_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    request: Option<Json<rest::CaptureContentAsKnowledgeRequest>>,
) -> Result<Json<ApiResponse<rest::KnowledgeResource>>, HttpError> {
    let resource = state
        .domain
        .capture_content_as_knowledge(
            user_id(&headers),
            id,
            request.and_then(|request| request.0.into_attribution()),
        )
        .await?;
    Ok(Json(ApiResponse::new(
        resource.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn report_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<rest::CreateReportRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::ContentReport>>), HttpError> {
    let report = state
        .domain
        .report_content(request.into_pb(user_id(&headers), id, idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            report.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn appeal_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<rest::CreateAppealRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::ContentAppeal>>), HttpError> {
    let appeal = state
        .domain
        .appeal_content(request.into_pb(user_id(&headers), id, idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            appeal.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn list_own_contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OwnContentQuery>,
) -> Result<Json<ApiResponse<rest::ContentPage>>, HttpError> {
    let page = state
        .domain
        .own_contents(
            bbs_link_pb::ListRequest {
                cursor: query.cursor,
                limit: query.limit,
                status: query.status.map(rest::ContentStatus::into_link),
                strategy: query.strategy,
                ids: None,
                author_id: None,
                content_type: query.content_type.map(rest::ContentType::into_link),
                domain: query.domain.map(rest::GrowthDomain::into_link),
                author_ids: Vec::new(),
            },
            user_id(&headers),
        )
        .await?;
    Ok(Json(ApiResponse::new(
        page.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_public_author_contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(author_id): Path<String>,
    Query(query): Query<PublicAuthorContentQuery>,
) -> Result<Json<ApiResponse<rest::ContentPage>>, HttpError> {
    let page = state
        .domain
        .public_author_contents(&user_id(&headers), &author_id, query.cursor, query.limit)
        .await?;
    Ok(Json(ApiResponse::new(
        page.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_own_appeals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::OwnAppealQuery>,
) -> Result<Json<ApiResponse<rest::AppealPage>>, HttpError> {
    let appeals = state
        .domain
        .own_appeals(query.into_pb(), user_id(&headers))
        .await?;
    Ok(Json(ApiResponse::new(
        appeals.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_moderation_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<audit_pb::ListReportsRequest>,
) -> Result<Json<ApiResponse<audit_pb::ReportPage>>, HttpError> {
    let _ = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.moderation_reports(request).await?,
    )))
}

async fn review_moderation_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(report_id): Path<String>,
    Json(mut request): Json<audit_pb::ReviewReportRequest>,
) -> Result<Json<ApiResponse<audit_pb::ContentReport>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state
            .domain
            .review_report({
                request.reviewer_id = reviewer_id;
                request.report_id = report_id;
                request
            })
            .await?,
    )))
}

async fn list_moderation_direct_message_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::ModerationDirectMessageReportQuery>,
) -> Result<Json<ApiResponse<rest::DirectMessageReportPage>>, HttpError> {
    let _ = moderator_id(&headers)?;
    let reports = state
        .domain
        .moderation_direct_message_reports(query.into_pb())
        .await?;
    Ok(Json(ApiResponse::new(
        reports.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn review_moderation_direct_message_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(report_id): Path<String>,
    Json(request): Json<rest::ReviewDirectMessageReportRequest>,
) -> Result<Json<ApiResponse<rest::DirectMessageReport>>, HttpError> {
    let reviewer_user_id = moderator_id(&headers)?;
    let report = state
        .domain
        .review_direct_message_report(request.into_pb(reviewer_user_id, report_id))
        .await?;
    Ok(Json(ApiResponse::new(
        report.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_moderation_comment_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::ModerationCommentReportQuery>,
) -> Result<Json<ApiResponse<rest::CommentReportPage>>, HttpError> {
    let _ = moderator_id(&headers)?;
    let reports = state
        .domain
        .moderation_comment_reports(query.into_pb())
        .await?;
    Ok(Json(ApiResponse::new(
        reports.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn review_moderation_comment_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(report_id): Path<String>,
    Json(request): Json<rest::ReviewCommentReportRequest>,
) -> Result<Json<ApiResponse<rest::CommentReport>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    let report = state
        .domain
        .review_comment_report(request.into_pb(reviewer_id, report_id))
        .await?;
    Ok(Json(ApiResponse::new(
        report.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_moderation_comment_appeals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::ModerationCommentAppealQuery>,
) -> Result<Json<ApiResponse<rest::CommentAppealPage>>, HttpError> {
    let _ = moderator_id(&headers)?;
    let appeals = state
        .domain
        .moderation_comment_appeals(query.into_pb())
        .await?;
    Ok(Json(ApiResponse::new(
        appeals.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn review_moderation_comment_appeal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(appeal_id): Path<String>,
    Json(request): Json<rest::ReviewCommentAppealRequest>,
) -> Result<Json<ApiResponse<rest::CommentAppeal>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    let appeal = state
        .domain
        .review_comment_appeal(request.into_pb(reviewer_id, appeal_id))
        .await?;
    Ok(Json(ApiResponse::new(
        appeal.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_moderation_appeals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<audit_pb::ListAppealsRequest>,
) -> Result<Json<ApiResponse<audit_pb::AppealPage>>, HttpError> {
    let _ = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state.domain.moderation_appeals(request).await?,
    )))
}

async fn review_moderation_appeal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(appeal_id): Path<String>,
    Json(mut request): Json<audit_pb::ReviewAppealRequest>,
) -> Result<Json<ApiResponse<audit_pb::ContentAppeal>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    Ok(Json(ApiResponse::new(
        state
            .domain
            .review_appeal({
                request.reviewer_id = reviewer_id;
                request.appeal_id = appeal_id;
                request
            })
            .await?,
    )))
}

async fn list_moderation_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(request): Query<rest::ModerationCommentQuery>,
) -> Result<Json<ApiResponse<rest::CommentPage>>, HttpError> {
    let _ = moderator_id(&headers)?;
    let comments = state.domain.moderation_comments(request.into_pb()).await?;
    Ok(Json(ApiResponse::new(
        comments.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn review_moderation_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(comment_id): Path<String>,
    Json(request): Json<rest::ReviewCommentRequest>,
) -> Result<Json<ApiResponse<rest::CommentItem>>, HttpError> {
    let reviewer_id = moderator_id(&headers)?;
    let comment = state
        .domain
        .review_moderation_comment(request.into_pb(reviewer_id, comment_id))
        .await?;
    Ok(Json(ApiResponse::new(
        comment.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn set_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<rest::SetReactionRequest>,
) -> Result<Json<ApiResponse<rest::Reaction>>, HttpError> {
    let (request, negative_feedback_reason, attribution) = request
        .into_pb(user_id(&headers), post_id)
        .map_err(HttpError::InvalidRequest)?;
    let reaction = state
        .domain
        .set_reaction(request, negative_feedback_reason, attribution)
        .await?;
    Ok(Json(ApiResponse::new(
        reaction.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn list_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Query(query): Query<rest::CommentQuery>,
) -> Result<Json<ApiResponse<rest::CommentPage>>, HttpError> {
    let comments = state
        .domain
        .comments(user_id(&headers), query.into_pb(post_id))
        .await?;
    Ok(Json(ApiResponse::new(
        comments.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn create_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<rest::CreateCommentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::CommentItem>>), HttpError> {
    let comment = state
        .domain
        .create_comment(request.into_pb(user_id(&headers), post_id, idempotency_key(&headers)))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            comment.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn delete_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((post_id, comment_id)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    state
        .domain
        .delete_comment(comment_pb::DeleteRequest {
            user_id: user_id(&headers),
            post_id,
            comment_id,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn accept_question_answer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((post_id, comment_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<rest::Content>>, HttpError> {
    let content = state
        .domain
        .accept_question_answer(user_id(&headers), post_id, comment_id)
        .await?;
    Ok(Json(ApiResponse::new(
        content.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn report_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((post_id, comment_id)): Path<(String, String)>,
    Json(request): Json<rest::CreateCommentReportRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::CommentReportReceipt>>), HttpError> {
    let report = state
        .domain
        .report_comment(
            user_id(&headers),
            request
                .into_pb(post_id, comment_id, idempotency_key(&headers))
                .map_err(HttpError::InvalidRequest)?,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            report.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn appeal_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(comment_id): Path<String>,
    Json(request): Json<rest::CreateCommentAppealRequest>,
) -> Result<(StatusCode, Json<ApiResponse<rest::CommentAppeal>>), HttpError> {
    let appeal = state
        .domain
        .appeal_comment(
            user_id(&headers),
            request
                .into_pb(comment_id, idempotency_key(&headers))
                .map_err(HttpError::InvalidRequest)?,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            appeal.try_into().map_err(HttpError::Contract)?,
        )),
    ))
}

async fn list_own_comment_appeals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<rest::OwnCommentAppealQuery>,
) -> Result<Json<ApiResponse<rest::CommentAppealPage>>, HttpError> {
    let appeals = state
        .domain
        .own_comment_appeals(query.into_pb(), user_id(&headers))
        .await?;
    Ok(Json(ApiResponse::new(
        appeals.try_into().map_err(HttpError::Contract)?,
    )))
}

async fn set_follow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    Json(request): Json<rest::FollowRequest>,
) -> Result<Json<ApiResponse<bbs_pb::SocialContext>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .follow(request.into_pb(user_id(&headers), target_user_id))
            .await?,
    )))
}

async fn set_relationship(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    Json(request): Json<rest::SetRelationshipRequest>,
) -> Result<Json<ApiResponse<bbs_pb::SocialContext>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .follow(request.into_pb(user_id(&headers), target_user_id))
            .await?,
    )))
}

async fn social_context(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<bbs_pb::SocialContext>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.social_context(user_id(&headers)).await?,
    )))
}

/// Follower lists are public profile facts: any signed-in reader may page
/// through them, so the path user is the list owner, not the caller.
async fn list_followers(
    State(state): State<AppState>,
    Path(target_user_id): Path<String>,
    Query(query): Query<rest::FollowerPageQuery>,
) -> Result<Json<ApiResponse<bbs_pb::FollowerPage>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .list_followers(query.into_pb(target_user_id))
            .await?,
    )))
}

async fn social_stats(
    State(state): State<AppState>,
    Path(target_user_id): Path<String>,
) -> Result<Json<ApiResponse<bbs_pb::SocialStats>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.social_stats(target_user_id).await?,
    )))
}

/// Co-walkers are read per viewer: the viewer identity drives the fail-closed
/// visibility filter, while the path route selects the shared facts.
async fn list_route_peers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(route_id): Path<String>,
    Query(query): Query<rest::RoutePeersQuery>,
) -> Result<Json<ApiResponse<bbs_pb::RoutePeerPage>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .list_route_peers(query.into_pb(user_id(&headers), route_id))
            .await?,
    )))
}

async fn list_route_participations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<bbs_pb::RouteParticipationList>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .list_route_participations(user_id(&headers))
            .await?,
    )))
}

async fn set_route_participation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(route_id): Path<String>,
    Json(request): Json<rest::RouteParticipationRequest>,
) -> Result<Json<ApiResponse<bbs_pb::RouteParticipationState>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .set_route_participation(request.into_pb(user_id(&headers), route_id))
            .await?,
    )))
}

async fn join_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(route_id): Path<String>,
    request: Option<Json<rest::JoinRouteRequest>>,
) -> Result<Json<ApiResponse<crate::domain::RouteJoinResult>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .join_route(
                user_id(&headers),
                route_id,
                request.and_then(|request| request.0.into_attribution()),
            )
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

fn merchant_admin_id(headers: &HeaderMap) -> Result<String, HttpError> {
    if !std::env::var("AUTH_REQUIRED").is_ok_and(|value| value == "true") {
        return Err(HttpError::Forbidden(
            "admin endpoints require AUTH_REQUIRED=true".to_string(),
        ));
    }
    let authorized = headers
        .get("x-user-roles")
        .and_then(|value| value.to_str().ok())
        .is_some_and(has_merchant_admin_role);
    if !authorized {
        return Err(HttpError::Forbidden(
            "merchant admin role is required".to_string(),
        ));
    }
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| HttpError::Forbidden("authenticated user is required".to_string()))
}

fn advertiser_admin_id(headers: &HeaderMap) -> Result<String, HttpError> {
    if !std::env::var("AUTH_REQUIRED").is_ok_and(|value| value == "true") {
        return Err(HttpError::Forbidden(
            "admin endpoints require AUTH_REQUIRED=true".to_string(),
        ));
    }
    let authorized = headers
        .get("x-user-roles")
        .and_then(|value| value.to_str().ok())
        .is_some_and(has_advertiser_admin_role);
    if !authorized {
        return Err(HttpError::Forbidden(
            "advertiser admin role is required".to_string(),
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

fn has_merchant_admin_role(roles: &str) -> bool {
    roles
        .split(',')
        .map(str::trim)
        .any(|role| matches!(role, "admin" | "merchant_admin"))
}

fn has_advertiser_admin_role(roles: &str) -> bool {
    roles
        .split(',')
        .map(str::trim)
        .any(|role| matches!(role, "admin" | "advertiser_admin"))
}

/// Delivery guardrail writes are a platform safety control, not merchant or
/// advertiser console functionality.
fn platform_admin_id(headers: &HeaderMap) -> Result<String, HttpError> {
    if !std::env::var("AUTH_REQUIRED").is_ok_and(|value| value == "true") {
        return Err(HttpError::Forbidden(
            "admin endpoints require AUTH_REQUIRED=true".to_string(),
        ));
    }
    let authorized = headers
        .get("x-user-roles")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|roles| {
            roles.split(',').map(str::trim).any(|role| role == "admin")
        });
    if !authorized {
        return Err(HttpError::Forbidden(
            "platform admin role is required".to_string(),
        ));
    }
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| HttpError::Forbidden("authenticated user is required".to_string()))
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn required_idempotency_key(headers: &HeaderMap) -> Result<String, HttpError> {
    idempotency_key(headers)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| HttpError::InvalidRequest("Idempotency-Key header is required".to_string()))
}

enum HttpError {
    Upstream(UpstreamError),
    InvalidRequest(String),
    Contract(String),
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
            Self::InvalidRequest(message) => {
                error_response(StatusCode::BAD_REQUEST, "invalid_request", message)
            }
            Self::Contract(message) => error_response(
                StatusCode::BAD_GATEWAY,
                "invalid_upstream_contract",
                message,
            ),
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
    use super::{
        has_advertiser_admin_role, has_merchant_admin_role, has_moderator_role,
        required_idempotency_key, user_id,
    };
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn moderator_roles_are_explicitly_allowlisted() {
        assert!(has_moderator_role("member, trust_safety"));
        assert!(has_moderator_role("admin"));
        assert!(!has_moderator_role("member, editor"));
        assert!(!has_moderator_role("supermoderator"));
    }

    #[test]
    fn merchant_admin_roles_are_explicitly_allowlisted() {
        assert!(has_merchant_admin_role("member, merchant_admin"));
        assert!(has_merchant_admin_role("admin"));
        assert!(!has_merchant_admin_role("merchant"));
        assert!(!has_merchant_admin_role("moderator"));
    }

    #[test]
    fn advertiser_admin_roles_are_explicitly_allowlisted() {
        assert!(has_advertiser_admin_role("member, advertiser_admin"));
        assert!(has_advertiser_admin_role("admin"));
        assert!(!has_advertiser_admin_role("merchant_admin"));
        assert!(!has_advertiser_admin_role("advertiser"));
    }

    #[test]
    fn fork_identity_and_idempotency_come_from_trusted_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-user-id", HeaderValue::from_static("member-1"));
        headers.insert("idempotency-key", HeaderValue::from_static("fork-1"));

        assert_eq!(user_id(&headers), "member-1");
        let key = match required_idempotency_key(&headers) {
            Ok(key) => key,
            Err(_) => panic!("idempotency key is present"),
        };
        assert_eq!(key, "fork-1");

        let missing_key = HeaderMap::new();
        assert!(required_idempotency_key(&missing_key).is_err());
    }
}
