use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use bookway_api::{
    ApiResponse, CommentDto, CommentPageDto, CommentQueryRequest, CreateCommentRequest,
    ErrorResponse, HealthResponse,
};
use tower_http::trace::TraceLayer;

use crate::domain::{CommentError, Domain};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) domain: Domain,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/posts/{post_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/v1/posts/{post_id}/comments/{comment_id}",
            delete(delete_comment),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("comment", addr, router(AppState { domain })).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "comment".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn list_comments(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Query(request): Query<CommentQueryRequest>,
) -> Result<Json<ApiResponse<CommentPageDto>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state.domain.list(&post_id, request).await?,
    )))
}

async fn create_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CommentDto>>), HttpError> {
    let comment = state
        .domain
        .create(
            &user_id(&headers),
            &post_id,
            request,
            idempotency_key(&headers),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(comment))))
}

async fn delete_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((post_id, comment_id)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    state
        .domain
        .delete(&user_id(&headers), &post_id, &comment_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn user_id(headers: &HeaderMap) -> String {
    headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

struct HttpError(CommentError);

impl From<CommentError> for HttpError {
    fn from(value: CommentError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            CommentError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            CommentError::Repository(crate::datasource::RepositoryError::ReplyDepthExceeded) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "reply_depth_exceeded")
            }
            CommentError::Repository(crate::datasource::RepositoryError::ParentNotFound(_)) => {
                (StatusCode::NOT_FOUND, "parent_comment_not_found")
            }
            CommentError::Repository(crate::datasource::RepositoryError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "comment_not_found")
            }
            CommentError::Repository(crate::datasource::RepositoryError::IdempotencyConflict) => {
                (StatusCode::CONFLICT, "idempotency_conflict")
            }
            CommentError::Repository(crate::datasource::RepositoryError::Database(_)) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            }
            CommentError::Repository(
                crate::datasource::RepositoryError::InvalidModerationState(_),
            ) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_moderation_state",
            ),
            CommentError::Repository(crate::datasource::RepositoryError::InvalidReplyHierarchy) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "invalid_reply_hierarchy")
            }
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
