use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bookway_api::{
    ApiResponse, ContentDto, CreateContentRequest, ErrorResponse, HealthResponse,
    UpdateContentRequest,
};
use tower_http::trace::TraceLayer;

use crate::domain::{ContentError, Domain};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) domain: Domain,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/posts", post(create_content))
        .route(
            "/v1/posts/{id}",
            get(get_public_content).patch(update_content),
        )
        .route("/v1/posts/{id}/publish", post(publish_content))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("bbs-link", addr, router(AppState { domain })).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs-link".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn get_public_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContentDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.domain.get_public(&id).await?)))
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
        .domain
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
            .domain
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
        state.domain.publish(&user_id(&headers), &id).await?,
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
            ContentError::Repository(crate::datasource::RepositoryError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "content_not_found")
            }
            ContentError::Repository(crate::datasource::RepositoryError::IdempotencyConflict(
                _,
            )) => (StatusCode::CONFLICT, "idempotency_conflict"),
            ContentError::Repository(
                crate::datasource::RepositoryError::Database(_)
                | crate::datasource::RepositoryError::Serialization(_)
                | crate::datasource::RepositoryError::InvalidTimestamp(_),
            ) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
            ContentError::Repository(crate::datasource::RepositoryError::VersionConflict) => {
                (StatusCode::CONFLICT, "version_conflict")
            }
            ContentError::Audit(_) => (StatusCode::BAD_GATEWAY, "audit_unavailable"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
