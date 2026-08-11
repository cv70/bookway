use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use bookway_api::{ApiResponse, HealthResponse};

use super::{
    api::{RankRequest, RankedItem},
    domain::RankService,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) rank: RankService,
}
pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/rank", post(rank))
        .with_state(state)
}
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "rank-main".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
async fn rank(
    State(state): State<AppState>,
    Json(request): Json<RankRequest>,
) -> Json<ApiResponse<Vec<RankedItem>>> {
    Json(ApiResponse::new(state.rank.rank(request)))
}
