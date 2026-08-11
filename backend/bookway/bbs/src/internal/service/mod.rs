use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use bookway_api::{
    ApiResponse, ErrorResponse, FollowRequest, HealthResponse, SocialContextDto,
    SocialContextRequest,
};
use tower_http::trace::TraceLayer;

use super::domain::{BbsError, BbsService};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) bbs: BbsService,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/social-context", get(context))
        .route("/internal/v1/users/{user_id}/follow", put(set_edge))
        .route("/v1/users/{user_id}/follow", put(set_edge))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn context(
    State(state): State<AppState>,
    Query(request): Query<SocialContextRequest>,
) -> Result<Json<ApiResponse<SocialContextDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.bbs.context(request).await?)))
}

async fn set_edge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    Json(request): Json<FollowRequest>,
) -> Result<Json<ApiResponse<SocialContextDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .bbs
            .set_edge(&user_id(&headers), &target_user_id, request)
            .await?,
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

struct HttpError(BbsError);

impl From<BbsError> for HttpError {
    fn from(value: BbsError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            BbsError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            BbsError::Repository(
                crate::internal::datasource::RepositoryError::BlockedRelationship,
            ) => (StatusCode::CONFLICT, "social_edge_conflict"),
            BbsError::Repository(crate::internal::datasource::RepositoryError::Database(_)) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            }
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
