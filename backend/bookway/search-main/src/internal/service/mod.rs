use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};
use serde::Deserialize;
use tower_http::trace::TraceLayer;

use super::{
    api::{SearchQueryRequest, SearchResponseDto, SuggestionResponseDto},
    datasource::SearchClientError,
    domain::{SearchMainError, SearchMainService},
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) search: SearchMainService,
}

#[derive(Debug, Deserialize)]
struct SuggestionQuery {
    q: String,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/search", get(search))
        .route("/internal/v1/suggestions", get(suggestions))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "search-main".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn search(
    State(state): State<AppState>,
    Query(request): Query<SearchQueryRequest>,
) -> Result<Json<ApiResponse<SearchResponseDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.search.search(request).await?)))
}

async fn suggestions(
    State(state): State<AppState>,
    Query(request): Query<SuggestionQuery>,
) -> Result<Json<ApiResponse<SuggestionResponseDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.search.suggestions(&request.q).await?,
    )))
}

struct HttpError(SearchMainError);

impl From<SearchMainError> for HttpError {
    fn from(value: SearchMainError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        match self.0 {
            SearchMainError::EmptyQuery | SearchMainError::QueryTooLong => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "invalid_search_query",
                    self.0.to_string(),
                )),
            )
                .into_response(),
            SearchMainError::Upstream(SearchClientError::Rejected {
                status,
                code,
                message,
            }) => (
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                Json(ErrorResponse::new(code, message)),
            )
                .into_response(),
            error @ SearchMainError::Upstream(SearchClientError::Transport(_)) => (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::new("search_unavailable", error.to_string())),
            )
                .into_response(),
        }
    }
}
