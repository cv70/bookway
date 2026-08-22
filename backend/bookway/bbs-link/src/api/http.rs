use crate::api::{ApiResponse, ErrorResponse, HealthResponse, pb};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
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
        .route("/v1/posts/{id}/fork", post(fork_route))
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
) -> Result<Json<ApiResponse<pb::Content>>, HttpError> {
    Ok(Json(ApiResponse::new(state.domain.get_public(&id).await?)))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<pb::CreateRequest>,
) -> Result<(StatusCode, Json<ApiResponse<pb::Content>>), HttpError> {
    request.user_id = user_id(&headers);
    request.idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content = state.domain.create(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(content))))
}

async fn update_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut request): Json<pb::UpdateRequest>,
) -> Result<Json<ApiResponse<pb::Content>>, HttpError> {
    request.user_id = user_id(&headers);
    request.id = id;
    Ok(Json(ApiResponse::new(state.domain.update(request).await?)))
}

async fn publish_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<pb::Content>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .publish(pb::PublishRequest {
                user_id: user_id(&headers),
                id,
                idempotency_key: headers
                    .get("idempotency-key")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string),
            })
            .await?,
    )))
}

async fn fork_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(source_route_id): Path<String>,
    Json(mut request): Json<pb::ForkRouteRequest>,
) -> Result<(StatusCode, Json<ApiResponse<pb::Content>>), HttpError> {
    request.user_id = user_id(&headers);
    request.source_route_id = source_route_id;
    request.idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let content = state.domain.fork_route(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(content))))
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
            ContentError::Repository(crate::datasource::DaoError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "content_not_found")
            }
            ContentError::Repository(crate::datasource::DaoError::IdempotencyConflict(_)) => {
                (StatusCode::CONFLICT, "idempotency_conflict")
            }
            ContentError::Repository(
                crate::datasource::DaoError::Database(_)
                | crate::datasource::DaoError::Serialization(_)
                | crate::datasource::DaoError::InvalidTimestamp(_)
                | crate::datasource::DaoError::InvalidContent(_),
            ) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
            ContentError::Repository(crate::datasource::DaoError::VersionConflict) => {
                (StatusCode::CONFLICT, "version_conflict")
            }
            ContentError::Audit(_) => (StatusCode::BAD_GATEWAY, "audit_unavailable"),
            ContentError::Media(_) => (StatusCode::BAD_GATEWAY, "media_unavailable"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
