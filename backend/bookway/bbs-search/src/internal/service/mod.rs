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
    domain::{SearchError, SearchService},
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) search: SearchService,
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
        service: "bbs-search".to_string(),
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

struct HttpError(SearchError);

impl From<SearchError> for HttpError {
    fn from(value: SearchError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            SearchError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            SearchError::Source(_) => (StatusCode::BAD_GATEWAY, "search_unavailable"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
