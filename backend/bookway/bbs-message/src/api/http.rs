use crate::api::{ApiResponse, ErrorResponse, HealthResponse, pb};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

use crate::domain::{Domain, MessageError};

#[derive(Clone)]
pub(crate) struct AppState {
    domain: Domain,
}

pub(crate) async fn serve(domain: Domain) -> Result<(), Box<dyn std::error::Error>> {
    let addr = domain.config.listen_addr;
    bookway_runtime::serve("bbs-message", addr, router(AppState { domain })).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/messages", post(send))
        .route("/v1/messages/conversations", get(list_conversations))
        .route(
            "/v1/messages/conversations/{conversation_id}",
            get(list_messages),
        )
        .route(
            "/v1/messages/conversations/{conversation_id}/read",
            post(mark_conversation_read),
        )
        .route(
            "/v1/message-preferences",
            get(get_preferences).put(update_preferences),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "bbs-message".to_string(),
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<pb::SendDirectMessageRequest>,
) -> Result<(StatusCode, Json<ApiResponse<pb::DirectMessage>>), HttpError> {
    request.sender_user_id = user_id(&headers);
    request.client_message_id = idempotency_key(&headers).unwrap_or_default();
    let message = state.domain.send(request).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(message))))
}

async fn list_conversations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut request): Query<pb::ListConversationsRequest>,
) -> Result<Json<ApiResponse<pb::ConversationPage>>, HttpError> {
    request.user_id = user_id(&headers);
    Ok(Json(ApiResponse::new(
        state.domain.list_conversations(request).await?,
    )))
}

async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    Query(mut request): Query<pb::ListMessagesRequest>,
) -> Result<Json<ApiResponse<pb::DirectMessagePage>>, HttpError> {
    request.user_id = user_id(&headers);
    request.conversation_id = conversation_id;
    Ok(Json(ApiResponse::new(
        state.domain.list_messages(request).await?,
    )))
}

async fn mark_conversation_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    Json(mut request): Json<pb::MarkConversationReadRequest>,
) -> Result<Json<ApiResponse<pb::MarkConversationReadResponse>>, HttpError> {
    request.user_id = user_id(&headers);
    request.conversation_id = conversation_id;
    Ok(Json(ApiResponse::new(
        state.domain.mark_conversation_read(request).await?,
    )))
}

async fn get_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<pb::DirectMessagePreferences>>, HttpError> {
    Ok(Json(ApiResponse::new(
        state
            .domain
            .get_preferences(pb::UserRequest {
                user_id: user_id(&headers),
            })
            .await?,
    )))
}

async fn update_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<pb::UpdateDirectMessagePreferencesRequest>,
) -> Result<Json<ApiResponse<pb::DirectMessagePreferences>>, HttpError> {
    request.user_id = user_id(&headers);
    Ok(Json(ApiResponse::new(
        state.domain.update_preferences(request).await?,
    )))
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
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

struct HttpError(MessageError);

impl From<MessageError> for HttpError {
    fn from(value: MessageError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            MessageError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            MessageError::Blocked => (StatusCode::FORBIDDEN, "direct_message_blocked"),
            MessageError::RecipientUnavailable => {
                (StatusCode::CONFLICT, "recipient_not_accepting_messages")
            }
            MessageError::SenderRestricted => {
                (StatusCode::FORBIDDEN, "direct_message_sender_restricted")
            }
            MessageError::UnderReview => (StatusCode::CONFLICT, "direct_message_under_review"),
            MessageError::Restricted => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "direct_message_restricted",
            ),
            MessageError::Audit(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "message_audit_unavailable")
            }
            MessageError::Dao(crate::datasource::DaoError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "conversation_not_found")
            }
            MessageError::Dao(crate::datasource::DaoError::NotParticipant) => {
                (StatusCode::FORBIDDEN, "conversation_access_denied")
            }
            MessageError::Dao(crate::datasource::DaoError::IdempotencyConflict) => {
                (StatusCode::CONFLICT, "idempotency_conflict")
            }
            MessageError::Upstream(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "social_graph_unavailable")
            }
            MessageError::Dao(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        };
        (status, Json(ErrorResponse::new(code, self.0.to_string()))).into_response()
    }
}
