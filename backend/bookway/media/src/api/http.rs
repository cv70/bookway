use crate::api::{ApiResponse, ErrorResponse, HealthResponse};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};

use crate::{
    api::pb,
    datasource::DaoError,
    domain::{Domain, MediaError},
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) domain: Domain,
}
pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/media/upload-url", post(create_upload))
        .route("/v1/media/{id}", get(get_media))
        .route("/v1/media/{id}/complete", post(complete_upload))
        .route("/v1/media/{id}/upload", put(proxy_upload))
        .with_state(state)
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("media", addr, router(AppState { domain })).await?;
    Ok(())
}
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "media".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
async fn create_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<pb::CreateUploadRequest>,
) -> Result<(StatusCode, Json<ApiResponse<pb::UploadResponse>>), HttpError> {
    request.user_id = user_id(&headers);
    let response = state.domain.create_upload(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(response))))
}
async fn proxy_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<ApiResponse<pb::MediaResource>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .proxy_upload(&user_id(&headers), &id, body)
            .await?,
    )))
}
async fn complete_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<pb::MediaResource>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .complete_upload(pb::ResourceRequest {
                user_id: user_id(&headers),
                id,
            })
            .await?,
    )))
}
async fn get_media(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<pb::MediaResource>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .get(pb::ResourceRequest {
                user_id: user_id(&headers),
                id,
            })
            .await?,
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
struct HttpError(MediaError);
impl From<MediaError> for HttpError {
    fn from(value: MediaError) -> Self {
        Self(value)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            MediaError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            MediaError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            MediaError::Repository(DaoError::NotFound) => {
                (StatusCode::NOT_FOUND, "media_not_found")
            }
            MediaError::Repository(DaoError::Database(_)) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            }
            MediaError::Object(_) => (StatusCode::BAD_GATEWAY, "object_storage_unavailable"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
