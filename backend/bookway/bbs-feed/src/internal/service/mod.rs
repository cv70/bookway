use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};
use tower_http::trace::TraceLayer;

use super::{
    api::{FeedDto, FeedQueryRequest},
    domain::{BbsFeedError, BbsFeedService},
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) feed: BbsFeedService,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/feed", get(feed))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs-feed".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn feed(
    State(state): State<AppState>,
    Query(request): Query<FeedQueryRequest>,
) -> Result<Json<ApiResponse<FeedDto>>, HttpError> {
    Ok(Json(ApiResponse::new(state.feed.feed(request).await?)))
}

struct HttpError(BbsFeedError);

impl From<BbsFeedError> for HttpError {
    fn from(value: BbsFeedError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse::new(
                "recommendation_unavailable",
                self.0.to_string(),
            )),
        )
            .into_response()
    }
}
