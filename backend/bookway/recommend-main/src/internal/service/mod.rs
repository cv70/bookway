use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use bookway_api::{ApiResponse, HealthResponse};
use tower_http::trace::TraceLayer;

use super::{
    api::{FeedDto, FeedQueryRequest},
    domain::FeedService,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) feed: FeedService,
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
        service: "recommend-main".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn feed(
    State(state): State<AppState>,
    Query(request): Query<FeedQueryRequest>,
) -> Json<ApiResponse<FeedDto>> {
    Json(ApiResponse::new(state.feed.recommend(request).await))
}
