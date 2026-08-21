use crate::api::{ApiResponse, HealthResponse, pb};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use tower_http::trace::TraceLayer;

use crate::domain::{Domain, InteractionStatusError};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) domain: Domain,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/posts/{post_id}/reactions", put(set_reaction))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("interaction-status", addr, router(AppState { domain })).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "interaction-status".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn set_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(mut request): Json<pb::SetReactionRequest>,
) -> Result<Json<ApiResponse<pb::Reaction>>, HttpError> {
    request.user_id = user_id(&headers);
    request.post_id = post_id;
    Ok(Json(ApiResponse::new(
        state.domain.set_reaction(request).await?,
    )))
}

struct HttpError(InteractionStatusError);
impl From<InteractionStatusError> for HttpError {
    fn from(value: InteractionStatusError) -> Self {
        Self(value)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            InteractionStatusError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_error"),
            InteractionStatusError::Dao(
                crate::datasource::DaoError::CachePeerRefresh,
            ) => (StatusCode::SERVICE_UNAVAILABLE, "context_cache_refreshing"),
            InteractionStatusError::Dao(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            }
        };
        (
            status,
            Json(crate::api::ErrorResponse::new(code, self.0.to_string())),
        )
            .into_response()
    }
}

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous")
        .to_string()
}
