use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};

use super::{
    api::{ContentAuditRequest, ContentAuditResponse},
    domain::{AuditError, AuditService},
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) audit: AuditService,
}
pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/internal/v1/audit", post(audit))
        .with_state(state)
}
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "content-audit".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
async fn audit(
    State(state): State<AppState>,
    Json(request): Json<ContentAuditRequest>,
) -> Result<Json<ApiResponse<ContentAuditResponse>>, HttpError> {
    Ok(Json(ApiResponse::new(state.audit.audit(request).await?)))
}

struct HttpError(AuditError);
impl From<AuditError> for HttpError {
    fn from(value: AuditError) -> Self {
        Self(value)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(
                "audit_persistence_failed",
                self.0.to_string(),
            )),
        )
            .into_response()
    }
}
