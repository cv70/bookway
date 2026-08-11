use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use bookway_api::{ApiResponse, HealthResponse};

use super::{
    api::{FeatureRequest, FeatureResponse},
    domain::FeatureService,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) features: FeatureService,
}
pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/features", post(features))
        .with_state(state)
}
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "feature-main".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
async fn features(
    State(state): State<AppState>,
    Json(request): Json<FeatureRequest>,
) -> Json<ApiResponse<FeatureResponse>> {
    Json(ApiResponse::new(state.features.features(request).await))
}
