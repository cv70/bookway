use crate::api::{ApiResponse, ErrorResponse, HealthResponse, pb};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

use crate::domain::{BbsError, Domain};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) domain: Domain,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/social/context", get(context))
        .route("/v1/social/{target_user_id}", post(set_edge))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("bbs", addr, router(AppState { domain })).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn context(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<pb::SocialContext>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .context(pb::ContextRequest {
                user_id: user_id(&headers),
                post_ids: Vec::new(),
            })
            .await?,
    )))
}

async fn set_edge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    Json(mut request): Json<pb::SetEdgeRequest>,
) -> Result<Json<ApiResponse<pb::SocialContext>>, HttpError> {
    request.user_id = user_id(&headers);
    request.target_user_id = target_user_id;
    Ok(Json(ApiResponse::new(
        state.domain.set_edge(request).await?,
    )))
}

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

struct HttpError(BbsError);

impl From<BbsError> for HttpError {
    fn from(value: BbsError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            BbsError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            BbsError::Dao(crate::datasource::DaoError::BlockedRelationship) => {
                (StatusCode::CONFLICT, "blocked_relationship")
            }
            BbsError::Dao(crate::datasource::DaoError::CachePeerRefresh) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "relationship_cache_refreshing",
            ),
            BbsError::Dao(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
