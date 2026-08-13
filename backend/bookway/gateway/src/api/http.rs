use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};
use serde::Deserialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use super::{
    ActionDto, CommentDto, ContentDto, CreateCommentRequest, CreateContentRequest,
    CreateJourneyRequest, FeedDto, FeedQueryRequest, FollowRequest, JourneyDto, MediaDto,
    MediaUploadRequest, MediaUploadResponse, ReactionDto, ReactionRequest, SearchQueryRequest,
    SearchResponseDto, SuggestionResponseDto, TodayDto, UpdateContentRequest,
    UserEventBatchRequest, UserEventIngestResponse,
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

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/journeys", get(list_journeys).post(create_journey))
        .route("/v1/today", get(today))
        .route("/v1/actions/{action_id}/complete", post(complete_action))
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
        .route("/v1/posts/{post_id}/reactions", put(set_reaction))
        .route(
            "/v1/posts/{post_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route("/v1/users/{user_id}/follow", put(set_follow))
        .with_state(state)
        .layer(CorsLayer::permissive())
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

async fn today(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<TodayDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.today(&user_id(&headers)).await?,
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
    Query(request): Query<SuggestionQuery>,
) -> Result<Json<ApiResponse<SuggestionResponseDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.suggestions(&request.q).await?,
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
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.domain.get_content(&id).await?)))
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
    Path(post_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<CommentDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.comments(&post_id).await?,
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
        .create_comment(&user_id(&headers), &post_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(comment))))
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

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("demo-user")
        .to_string()
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

struct HttpError(UpstreamError);

impl From<UpstreamError> for HttpError {
    fn from(value: UpstreamError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        match self.0 {
            error @ UpstreamError::Transport { .. } => (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::new(
                    "upstream_unavailable",
                    error.to_string(),
                )),
            )
                .into_response(),
        }
    }
}
