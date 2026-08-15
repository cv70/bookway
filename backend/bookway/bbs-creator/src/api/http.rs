use crate::api::{ApiResponse, ErrorResponse, HealthResponse, pb};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use tower_http::trace::TraceLayer;

use crate::domain::{CreatorError, Domain};

#[derive(Clone)]
pub(crate) struct AppState {
    domain: Domain,
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("bbs-creator", addr, router(AppState { domain })).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/creators", get(list_profiles))
        .route("/v1/creators/{user_id}", get(get_profile))
        .route("/v1/creator-profile", put(upsert_profile))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs-creator".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn get_profile(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<ApiResponse<pb::CreatorProfile>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .get_profile(pb::CreatorProfileRequest { user_id })
            .await?,
    )))
}

async fn list_profiles(
    State(state): State<AppState>,
    Query(request): Query<pb::ListCreatorProfilesRequest>,
) -> Result<Json<ApiResponse<pb::CreatorProfilePage>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.list_profiles(request).await?,
    )))
}

async fn upsert_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<pb::UpsertCreatorProfileRequest>,
) -> Result<Json<ApiResponse<pb::CreatorProfile>>, HttpError> {
    request.user_id = user_id(&headers);
    Ok(Json(ApiResponse::new(
        state.domain.upsert_profile(request).await?,
    )))
}

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

struct HttpError(CreatorError);

impl From<CreatorError> for HttpError {
    fn from(value: CreatorError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            CreatorError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            CreatorError::Repository(crate::datasource::RepositoryError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "creator_not_found")
            }
            CreatorError::Repository(crate::datasource::RepositoryError::HandleTaken(_)) => {
                (StatusCode::CONFLICT, "handle_taken")
            }
            CreatorError::Repository(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
