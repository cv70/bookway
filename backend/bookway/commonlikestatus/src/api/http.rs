use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use bookway_api::{ApiResponse, HealthResponse, ReactionDto, ReactionRequest};
use tower_http::trace::TraceLayer;

use crate::domain::{Domain, LikeStatusError};

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
    bookway_runtime::serve("commonlikestatus", addr, router(AppState { domain })).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "commonlikestatus".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn set_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<ReactionRequest>,
) -> Result<Json<ApiResponse<ReactionDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .set_reaction(&user_id(&headers), &post_id, request)
            .await?,
    )))
}

struct HttpError(LikeStatusError);
impl From<LikeStatusError> for HttpError {
    fn from(value: LikeStatusError) -> Self {
        Self(value)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(bookway_api::ErrorResponse::new(
                "storage_error",
                self.0.to_string(),
            )),
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
