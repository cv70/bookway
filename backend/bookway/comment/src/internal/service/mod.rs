use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use bookway_api::{ApiResponse, CommentDto, CreateCommentRequest, ErrorResponse, HealthResponse};
use tower_http::trace::TraceLayer;

use super::domain::{CommentError, CommentService};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) comment: CommentService,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/posts/{post_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/internal/v1/posts/{post_id}/comments",
            get(list_comments).post(create_comment),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
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
) -> Result<Json<ApiResponse<Vec<CommentDto>>>, HttpError> {
    Ok(Json(ApiResponse::new(state.comment.list(&post_id).await?)))
}

async fn create_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
    Json(request): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CommentDto>>), HttpError> {
    let comment = state
        .comment
        .create(&user_id(&headers), &post_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(comment))))
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
            CommentError::Repository(super::datasource::RepositoryError::ParentNotFound(_)) => {
                (StatusCode::NOT_FOUND, "parent_comment_not_found")
            }
            CommentError::Repository(super::datasource::RepositoryError::Database(_)) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            }
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
