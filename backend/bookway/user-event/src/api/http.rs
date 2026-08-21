use crate::api::{ApiResponse, ErrorResponse, HealthResponse};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

use super::pb;
use crate::domain::{IngestError, UserEventService};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) events: UserEventService,
}

pub(crate) async fn serve(domain: crate::domain::Domain) -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::serve(
        "user-event",
        domain.config.listen_addr,
        router(AppState {
            events: domain.events,
        }),
    )
    .await?;
    Ok(())
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/events", post(ingest))
        .with_state(state)
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "user-event".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<pb::IngestRequest>,
) -> Result<Json<ApiResponse<pb::IngestResponse>>, HttpError> {
    request.user_id = headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    Ok(Json(ApiResponse::new(state.events.ingest(request).await?)))
}

struct HttpError(IngestError);

impl From<IngestError> for HttpError {
    fn from(value: IngestError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            IngestError::MissingUser => (StatusCode::UNAUTHORIZED, "missing_identity"),
            IngestError::EmptyBatch | IngestError::BatchTooLarge => {
                (StatusCode::BAD_REQUEST, "invalid_event_batch")
            }
            IngestError::Dao(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
