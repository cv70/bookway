use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};
use tower_http::trace::TraceLayer;

use super::{
    api::{ActionDto, CreateJourneyRequest, JourneyDto, TodayDto},
    datasource::RepositoryError,
    domain::{GrowthError, GrowthService},
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) growth: GrowthService,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/internal/v1/journeys",
            get(list_journeys).post(create_journey),
        )
        .route("/internal/v1/today", get(today))
        .route(
            "/internal/v1/actions/{action_id}/complete",
            post(complete_action),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "growth".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn list_journeys(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ApiResponse<Vec<JourneyDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.growth.list_journeys(&user_id(&headers)).await?,
    )))
}

async fn create_journey(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateJourneyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<JourneyDto>>), HttpError> {
    let journey = state
        .growth
        .create_journey(&user_id(&headers), request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(journey))))
}

async fn today(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ApiResponse<TodayDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.growth.today(&user_id(&headers)).await?,
    )))
}

async fn complete_action(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(action_id): Path<String>,
) -> Result<Json<ApiResponse<ActionDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .growth
            .complete_action(&user_id(&headers), &action_id)
            .await?,
    )))
}

fn user_id(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("demo-user")
        .to_string()
}

struct HttpError(GrowthError);

impl From<GrowthError> for HttpError {
    fn from(value: GrowthError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            GrowthError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            GrowthError::Repository(RepositoryError::ActionNotFound(_)) => {
                (StatusCode::NOT_FOUND, "action_not_found")
            }
            GrowthError::Repository(
                RepositoryError::Database(_) | RepositoryError::Serialization(_),
            ) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
