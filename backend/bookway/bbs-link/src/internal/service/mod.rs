use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bookway_api::{
    ApiResponse, ContentDto, ContentPageDto, ContentQueryRequest, CreateContentRequest,
    ErrorResponse, HealthResponse, UpdateContentRequest,
};
use tower_http::trace::TraceLayer;

use super::domain::{ContentError, ContentService};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) content: ContentService,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/contents", get(list_contents))
        .route("/internal/v1/contents/{id}", get(get_internal_content))
        .route("/internal/v1/posts", post(create_content))
        .route(
            "/internal/v1/posts/{id}",
            get(get_public_content).patch(update_content),
        )
        .route("/internal/v1/posts/{id}/publish", post(publish_content))
        .route("/v1/posts", post(create_content))
        .route(
            "/v1/posts/{id}",
            get(get_public_content).patch(update_content),
        )
        .route("/v1/posts/{id}/publish", post(publish_content))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs-link".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn list_contents(
    State(state): State<AppState>,
    Query(query): Query<ContentQueryRequest>,
) -> Result<Json<ApiResponse<ContentPageDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.content.list(query).await?)))
}

async fn get_internal_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.content.get(&id).await?)))
}

async fn get_public_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.content.get_public(&id).await?)))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateContentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ContentDto>>), HttpError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content = state
        .content
        .create(&user_id(&headers), request, key)
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
            .content
            .update(&user_id(&headers), &id, request)
            .await?,
    )))
}

async fn publish_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.content.publish(&user_id(&headers), &id).await?,
    )))
}

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

struct HttpError(ContentError);

impl From<ContentError> for HttpError {
    fn from(value: ContentError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            ContentError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            ContentError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ContentError::Repository(super::datasource::RepositoryError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "content_not_found")
            }
            ContentError::Repository(super::datasource::RepositoryError::IdempotencyConflict(
                _,
            )) => (StatusCode::CONFLICT, "idempotency_conflict"),
            ContentError::Repository(
                super::datasource::RepositoryError::Database(_)
                | super::datasource::RepositoryError::Serialization(_)
                | super::datasource::RepositoryError::InvalidTimestamp(_),
            ) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
            ContentError::Repository(super::datasource::RepositoryError::VersionConflict) => {
                (StatusCode::CONFLICT, "version_conflict")
            }
            ContentError::Audit(_) => (StatusCode::BAD_GATEWAY, "audit_unavailable"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
